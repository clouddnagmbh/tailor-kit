// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Commit-level building blocks (wave 15, strand D; types in `tk_git_types::plumbing`):
//! [`Repo::commit_info`] reads a commit in full, [`Repo::pick`] replays one commit's change
//! onto another in memory (the merge of cherry-pick and revert, so the product's files go
//! through its `FileMerger`), [`Repo::write_tree`] and [`Repo::write_commit`] write objects.
//!
//! None of them moves a ref, touches the working tree or the index; objects nothing points at
//! are harmless and go with `git gc`. What builds on them moves a branch only
//! through `GitAction::Rewrite` (`rewrite_branch.rs`).

use base64::Engine as _;
use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::Mode;
use tk_git_types::GitIdentity;
use tk_git_types::plumbing::{
    GitCommitDetails, GitCommitWrite, GitPick, GitPickResult, GitSignature, GitTreeContent,
    GitTreeWrite,
};

use crate::merge::{MergeKind, ThreeWay};
use crate::repo::Repo;
use crate::util::{Context, Result, fail, flatten};

fn to_signature(s: gix::actor::SignatureRef<'_>) -> Result<GitSignature> {
    let time = s.time().ctx("cannot read a commit")?;
    Ok(GitSignature {
        name: s.name.to_str_lossy().into_owned(),
        email: s.email.to_str_lossy().into_owned(),
        time: time.seconds,
        offset: time.offset / 60,
    })
}

fn from_signature(s: &GitSignature) -> gix::actor::Signature {
    gix::actor::Signature {
        name: s.name.as_str().into(),
        email: s.email.as_str().into(),
        time: gix::date::Time::new(s.time, s.offset * 60),
    }
}

impl Repo {
    /// A commit in full (`GitApi.commitInfo`).
    pub fn commit_info(&self, rev: &str) -> Result<GitCommitDetails> {
        let id = self.resolve_rev(rev)?;
        let commit = self.repo.find_commit(id).ctx("cannot read a commit")?;
        let decoded = commit.decode().ctx("cannot read a commit")?;
        let signed = crate::signing::has_signature(&decoded);
        Ok(GitCommitDetails {
            oid: id.to_string(),
            tree: commit.tree_id().ctx("cannot read a commit")?.to_string(),
            parents: commit.parent_ids().map(|p| p.to_string()).collect(),
            message: decoded.message.to_str_lossy().into_owned(),
            author: to_signature(decoded.author().ctx("cannot read a commit")?)?,
            committer: to_signature(decoded.committer().ctx("cannot read a commit")?)?,
            signed,
        })
    }

    /// Replays the change of `pick.commit` onto `pick.onto` in memory
    /// (`GitApi.pick`). The tree is written; conflicted paths hold `onto`'s version in it.
    pub fn pick(&self, pick: &GitPick) -> Result<GitPickResult> {
        let onto = self.resolve_rev(&pick.onto)?;
        let commit = self.resolve_rev(&pick.commit)?;
        let parents: Vec<ObjectId> = self
            .repo
            .find_commit(commit)
            .ctx("cannot read a commit")?
            .parent_ids()
            .map(|p| p.detach())
            .collect();
        let parent = match (parents.len(), pick.mainline) {
            (0, _) => None,
            (1, _) => Some(parents[0]),
            (n, Some(m)) if m >= 1 && (m as usize) <= n => Some(parents[m as usize - 1]),
            (_, Some(_)) => return Err(fail("the commit has no such parent")),
            (_, None) => {
                return Err(fail(
                    "this is a merge commit: say which parent counts as the mainline",
                ));
            }
        };
        let label = commit.to_hex_with_len(7).to_string();
        let tw = if pick.revert {
            let parent = parent.ok_or_else(|| fail("the first commit cannot be reverted"))?;
            crate::revert::three_way(onto, commit, parent, &label)
        } else {
            ThreeWay {
                kind: MergeKind::CherryPick,
                base: parent,
                ours: onto,
                theirs: commit,
                label: &label,
            }
        };
        let computed = self.compute_merge(&self.repo, &tw)?;
        let (tree, conflicts, other_conflicts) = self.pick_outcome(computed)?;
        Ok(GitPickResult {
            tree: tree.to_string(),
            clean: conflicts.is_empty() && other_conflicts.is_empty(),
            conflicts,
            other_conflicts,
        })
    }

    /// A tree id or a rev (peeled to its tree).
    fn tree_of(&self, rev: &str) -> Result<ObjectId> {
        let id = self
            .repo
            .rev_parse_single(rev.trim().as_bytes().as_bstr())
            .map_err(|e| fail(format!("unknown revision {rev:?}: {e}")))?;
        let tree = id
            .object()
            .ctx("cannot read an object")?
            .peel_to_tree()
            .map_err(|_| fail(format!("{rev:?} does not name a tree or commit")))?;
        Ok(tree.id)
    }

    /// Writes `write.base` with `write.files` written or removed (`GitApi.writeTree`).
    pub fn write_tree(&self, write: &GitTreeWrite) -> Result<String> {
        let base = self.tree_of(&write.base)?;
        let mut editor = self.repo.edit_tree(base).ctx("cannot build the tree")?;
        let current = flatten(&self.repo, base)?;
        for file in &write.files {
            let rel = file.path.strip_prefix('/').unwrap_or(&file.path);
            if rel.is_empty()
                || rel
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
            {
                return Err(fail(format!(
                    "{:?} is not a path in the repository",
                    file.path
                )));
            }
            let path = BString::from(rel);
            let existing = current.get(&path).map(|b| b.mode);
            let mode_for = |from: Option<Mode>| match (file.executable, from) {
                (Some(true), _) => Mode::FILE_EXECUTABLE,
                (Some(false), _) => Mode::FILE,
                (None, Some(m)) => m,
                (None, None) => Mode::FILE,
            };
            let blob = |bytes: &[u8]| -> Result<ObjectId> {
                Ok(self
                    .repo
                    .write_blob(bytes)
                    .ctx("cannot write a file")?
                    .detach())
            };
            match &file.content {
                GitTreeContent::Text { text } => {
                    let id = blob(text.as_bytes())?;
                    editor
                        .upsert(
                            path.as_bstr(),
                            crate::commit::entry_kind(mode_for(existing)),
                            id,
                        )
                        .ctx("cannot build the tree")?;
                }
                GitTreeContent::Base64 { data } => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|_| fail(format!("{} holds invalid base64", file.path)))?;
                    let id = blob(&bytes)?;
                    editor
                        .upsert(
                            path.as_bstr(),
                            crate::commit::entry_kind(mode_for(existing)),
                            id,
                        )
                        .ctx("cannot build the tree")?;
                }
                GitTreeContent::From { rev } => {
                    let other = flatten(&self.repo, self.tree_of(rev)?)?;
                    match other.get(&path) {
                        Some(b) => {
                            let mode = if file.executable.is_some() {
                                mode_for(Some(b.mode))
                            } else {
                                b.mode
                            };
                            editor
                                .upsert(path.as_bstr(), crate::commit::entry_kind(mode), b.id)
                                .ctx("cannot build the tree")?;
                        }
                        None => {
                            editor.remove(path.as_bstr()).ctx("cannot build the tree")?;
                        }
                    }
                }
                GitTreeContent::Delete => {
                    editor.remove(path.as_bstr()).ctx("cannot build the tree")?;
                }
            }
        }
        Ok(editor.write().ctx("cannot write the tree")?.to_string())
    }

    /// Writes a commit object (`GitApi.writeCommit`); no ref moves. Missing signatures: the
    /// Git identity, else `fallback`, now.
    pub fn write_commit(
        &self,
        write: &GitCommitWrite,
        fallback: Option<&GitIdentity>,
    ) -> Result<String> {
        let tree = ObjectId::from_hex(write.tree.trim().as_bytes())
            .map_err(|_| fail(format!("{:?} is not a tree id", write.tree)))?;
        self.repo
            .find_tree(tree)
            .map_err(|_| fail(format!("the tree {} does not exist", write.tree)))?;
        let parents = write
            .parents
            .iter()
            .map(|p| self.resolve_rev(p))
            .collect::<Result<Vec<_>>>()?;
        let now = || -> Result<gix::actor::Signature> {
            Ok(crate::commit::signature(&self.identity_or(fallback)?))
        };
        let committer = match &write.committer {
            Some(s) => from_signature(s),
            None => now()?,
        };
        let author = match &write.author {
            Some(s) => from_signature(s),
            None => committer.clone(),
        };
        let commit = gix::objs::Commit {
            tree,
            parents: parents.into(),
            author,
            committer,
            encoding: None,
            message: write.message.as_str().into(),
            extra_headers: Vec::new(),
        };
        Ok(self.write_commit_object(commit)?.to_string())
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `commit` (selected paths only, or concluding a merge) and `log`.

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::index::entry::{Flags, Mode, Stage};
use gix::objs::tree::EntryKind;
use rt_app::git::{GitCommit, GitIdentity};

use crate::repo::Repo;
use crate::util::{Context, Result, fail};
use crate::worktree::stat_of;

pub fn entry_kind(mode: Mode) -> EntryKind {
    match mode {
        Mode::FILE_EXECUTABLE => EntryKind::BlobExecutable,
        Mode::SYMLINK => EntryKind::Link,
        Mode::COMMIT => EntryKind::Commit,
        _ => EntryKind::Blob,
    }
}

fn index_mode(kind: EntryKind) -> Mode {
    match kind {
        EntryKind::BlobExecutable => Mode::FILE_EXECUTABLE,
        EntryKind::Link => Mode::SYMLINK,
        EntryKind::Commit => Mode::COMMIT,
        EntryKind::Tree => Mode::DIR,
        EntryKind::Blob => Mode::FILE,
    }
}

/// A signature for now.
pub fn signature(identity: &GitIdentity) -> gix::actor::Signature {
    gix::actor::Signature {
        name: identity.name.as_str().into(),
        email: identity.email.as_str().into(),
        time: gix::date::Time::now_local_or_utc(),
    }
}

/// `GitCommit` of a commit in `repo`.
pub fn describe(repo: &gix::Repository, id: ObjectId) -> Result<GitCommit> {
    let commit = repo.find_commit(id).ctx("cannot read a commit")?;
    let decoded = commit.decode().ctx("cannot read a commit")?;
    let author = decoded.author().ctx("cannot read a commit")?;
    Ok(GitCommit {
        oid: id.to_string(),
        summary: decoded.message().summary().to_str_lossy().into_owned(),
        author: author.name.to_str_lossy().into_owned(),
        time: author.seconds(),
    })
}

impl Repo {
    /// Commits exactly `paths` (new, changed or deleted files of the collection) as they are
    /// in the working tree, on top of `HEAD`. While a merge is in progress the merge result
    /// (the index) is the base and the commit gets both parents; `paths` may then be empty.
    /// The identity comes from the Git configuration, else `fallback`.
    pub fn commit(
        &self,
        message: &str,
        paths: &[String],
        fallback: Option<&GitIdentity>,
    ) -> Result<GitCommit> {
        let message = message.trim();
        if message.is_empty() {
            return Err(fail("the commit message is empty"));
        }
        // A merge, cherry-pick or revert that stopped on conflicts; only a merge adds a parent.
        let underway = self.underway()?.is_some();
        let merge_parent = self.merge_head()?;
        let mut index = self.load_index()?;
        if underway {
            // A conflicted file that is committed counts as resolved with its working-tree
            // content (like `git add`); all others must be resolved first.
            let selected = paths
                .iter()
                .map(|p| self.scope.abs(p))
                .collect::<Result<std::collections::BTreeSet<_>>>()?;
            if index
                .entries()
                .iter()
                .any(|e| e.stage() != Stage::Unconflicted && !selected.contains(e.path(&index)))
            {
                return Err(fail("resolve all conflicts before committing the merge"));
            }
        } else if paths.is_empty() {
            return Err(fail("select the files to commit"));
        }
        let identity = self.identity_or(fallback)?;
        let head = self.head_commit()?;
        let head_tree = self.head_tree()?;

        let base = if underway {
            self.tree_from_index(&index)?
        } else {
            head_tree
        };
        let mut editor = self.repo.edit_tree(base).ctx("cannot read the HEAD tree")?;
        let (mut pipeline, _) = self
            .repo
            .filter_pipeline(None)
            .ctx("cannot read the attributes")?;
        let mut updates = Vec::new();
        for path in paths {
            let repo_path = self.scope.abs(path)?;
            match pipeline
                .worktree_file_to_object(repo_path.as_bstr(), &index)
                .ctx("cannot read a file")?
            {
                Some((_, EntryKind::Commit, _)) => {
                    return Err(fail(format!("{path} is a nested repository")));
                }
                Some((id, kind, _)) => {
                    editor
                        .upsert(repo_path.as_bstr(), kind, id)
                        .ctx("cannot build the tree")?;
                    updates.push((repo_path, Some((id, kind))));
                }
                None => {
                    let file = crate::worktree::fs_path(&self.workdir, repo_path.as_bstr())?;
                    if file.is_dir() {
                        return Err(fail(format!("{path} is a folder, not a file")));
                    }
                    editor
                        .remove(repo_path.as_bstr())
                        .ctx("cannot build the tree")?;
                    updates.push((repo_path, None));
                }
            }
        }
        let tree = editor.write().ctx("cannot write the tree")?.detach();
        if !underway && tree == head_tree {
            return Err(fail("nothing to commit: the selected files are unchanged"));
        }
        let signature = signature(&identity);
        let mut time = gix::date::parse::TimeBuf::default();
        let sig = signature.to_ref(&mut time);
        let parents: Vec<ObjectId> = head.into_iter().chain(merge_parent).collect();
        let id = self
            .repo
            .commit_as(sig, sig, "HEAD", message, tree, parents)
            .ctx("cannot commit")?
            .detach();

        // Keep the index in step for exactly the committed paths.
        for (path, _) in &updates {
            index.remove_entries(|_, p, _| p == path.as_bstr());
        }
        for (path, object) in updates {
            if let Some((oid, kind)) = object {
                let stat = stat_of(&crate::worktree::fs_path(&self.workdir, path.as_bstr())?);
                index.dangerously_push_entry(
                    stat,
                    oid,
                    Flags::empty(),
                    index_mode(kind),
                    path.as_bstr(),
                );
            }
        }
        self.write_index(&mut index)?;
        if underway {
            self.clear_merge_state()?;
        }
        describe(&self.repo, id)
    }

    /// Tree of the index's unconflicted entries.
    pub(crate) fn tree_from_index(&self, index: &gix::index::State) -> Result<ObjectId> {
        let empty = ObjectId::empty_tree(self.repo.object_hash());
        let mut editor = self.repo.edit_tree(empty).ctx("cannot build the tree")?;
        for entry in index.entries() {
            if entry.stage() == Stage::Unconflicted {
                editor
                    .upsert(entry.path(index), entry_kind(entry.mode), entry.id)
                    .ctx("cannot build the tree")?;
            }
        }
        Ok(editor.write().ctx("cannot write the tree")?.detach())
    }

    /// Commits of the current branch, newest first (by commit time).
    pub fn log(&self, limit: u32) -> Result<Vec<GitCommit>> {
        let Some(head) = self.head_commit()? else {
            return Ok(Vec::new());
        };
        let walk = self
            .repo
            .rev_walk([head])
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                Default::default(),
            ))
            .all()
            .ctx("cannot walk the history")?;
        let mut out = Vec::new();
        for info in walk.take(limit as usize) {
            let info = info.ctx("cannot walk the history")?;
            out.push(describe(&self.repo, info.id)?);
        }
        Ok(out)
    }
}

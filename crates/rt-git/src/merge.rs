// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Merging the upstream (pull), and resolving conflicts afterwards.

use std::collections::BTreeSet;

use gix::ObjectId;
use gix::bstr::BStr;
use gix::index::entry::{Flags, Stage};
use gix::merge::tree::{TreatAsUnresolved, apply_index_entries::RemovalMode};
use rt_app::git::{GitConflict, GitIdentity, GitPullResult, GitResolution};

use crate::commit::signature;
use crate::repo::Repo;
use crate::util::{Blob, Context, Result, blob_text, fail};
use crate::worktree::{Probe, stat_of, switch};

impl Repo {
    /// Merges `theirs` into `ours` (the current branch, at `HEAD`). Without conflicts: a merge
    /// commit. With conflicts: the merge stops (`MERGE_HEAD`, conflict stages in the index,
    /// conflict markers in the files).
    pub(crate) fn merge(
        &self,
        ours: ObjectId,
        theirs: ObjectId,
        their_label: &str,
        identity: &GitIdentity,
    ) -> Result<GitPullResult> {
        let branch = self.head_branch()?.unwrap_or_else(|| "HEAD".into());
        let labels = gix::merge::blob::builtin_driver::text::Labels {
            ancestor: Some(BStr::new("base")),
            current: Some(BStr::new(branch.as_str())),
            other: Some(BStr::new(their_label)),
        };
        // No rename tracking: a conflict is always about one path, the way the UI shows it.
        let options = self
            .repo
            .tree_merge_options()
            .ctx("cannot read the merge configuration")?
            .with_rewrites(None);
        let mut outcome = self
            .repo
            .merge_commits(ours, theirs, labels, options.into())
            .map_err(|e| fail(format!("cannot merge {their_label}: {e}")))?;
        let how = TreatAsUnresolved::git();
        let conflicted = outcome.tree_merge.has_unresolved_conflicts(how);
        let tree = outcome
            .tree_merge
            .tree
            .write()
            .ctx("cannot write the merged tree")?
            .detach();
        let message = format!("Merge {their_label} into {branch}");
        let mut index = switch(self, self.commit_tree(ours)?, tree, "pull")?;
        if !conflicted {
            let sig = signature(identity);
            let mut time = gix::date::parse::TimeBuf::default();
            let sig = sig.to_ref(&mut time);
            let id = self
                .repo
                .commit_as(sig, sig, "HEAD", &message, tree, [ours, theirs])
                .ctx("cannot commit the merge")?
                .detach();
            self.write_index(&mut index)?;
            return Ok(GitPullResult::Merged { oid: id.to_string() });
        }
        outcome
            .tree_merge
            .index_changed_after_applying_conflicts(&mut index, how, RemovalMode::Prune);
        self.write_index(&mut index)?;
        let git_dir = self.git_dir();
        let io = |e: std::io::Error| fail(format!("cannot record the merge: {e}"));
        std::fs::write(git_dir.join("MERGE_HEAD"), format!("{theirs}\n")).map_err(io)?;
        std::fs::write(git_dir.join("MERGE_MSG"), format!("{message}\n")).map_err(io)?;
        std::fs::write(git_dir.join("MERGE_MODE"), "").map_err(io)?;
        let paths: BTreeSet<String> = index
            .entries()
            .iter()
            .filter(|e| e.stage() != Stage::Unconflicted)
            .filter_map(|e| self.scope.rel(e.path(&index)))
            .collect();
        Ok(GitPullResult::Conflicts {
            paths: paths.into_iter().collect(),
        })
    }

    fn stage_blob(&self, index: &gix::index::State, path: &BStr, stage: Stage) -> Option<Blob> {
        index
            .entry_by_path_and_stage(path, stage)
            .map(|e| Blob { mode: e.mode, id: e.id })
    }

    /// Base, ours and theirs of a conflicted file.
    pub fn conflict(&self, path: &str) -> Result<GitConflict> {
        let repo_path = self.scope.abs(path)?;
        let index = self.load_index()?;
        let side = |stage| self.stage_blob(&index, repo_path.as_ref(), stage);
        let (base, ours, theirs) = (side(Stage::Base), side(Stage::Ours), side(Stage::Theirs));
        if base.is_none() && ours.is_none() && theirs.is_none() {
            return Err(fail(format!("{path} has no conflict")));
        }
        let text = |b: Option<Blob>| b.map(|b| blob_text(&self.repo, b.id)).transpose();
        Ok(GitConflict {
            path: path.to_owned(),
            base: text(base)?,
            ours: text(ours)?,
            theirs: text(theirs)?,
        })
    }

    /// Resolves a conflicted file with one side: writes it to the working tree (or deletes
    /// the file if that side deleted it) and marks it resolved in the index.
    pub fn resolve(&self, path: &str, resolution: GitResolution) -> Result<()> {
        let repo_path = self.scope.abs(path)?;
        let mut index = self.load_index()?;
        let conflicted = index
            .entries()
            .iter()
            .any(|e| e.stage() != Stage::Unconflicted && e.path(&index) == repo_path);
        if !conflicted {
            return Err(fail(format!("{path} has no conflict")));
        }
        let stage = match resolution {
            GitResolution::Ours => Stage::Ours,
            GitResolution::Theirs => Stage::Theirs,
        };
        let chosen = self.stage_blob(&index, repo_path.as_ref(), stage);
        index.remove_entries(|_, p, _| p == repo_path);
        match chosen {
            Some(blob) => {
                let file = Probe::new(self)?.write(repo_path.as_ref(), blob)?;
                index.dangerously_push_entry(stat_of(&file), blob.id, Flags::empty(), blob.mode, repo_path.as_ref());
            }
            None => {
                let file = crate::worktree::fs_path(&self.workdir, repo_path.as_ref())?;
                match std::fs::remove_file(&file) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(fail(format!("cannot delete {path}: {e}"))),
                }
            }
        }
        self.write_index(&mut index)
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Committing files onto a branch without a checkout: the new tree is the branch's (or a start
//! revision's) tree with the given files written or removed, and only the branch ref moves.
//! The working tree, the index and `HEAD` stay untouched, so a tool can put a contribution on
//! its own branch while the user keeps working (since wave 13).

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::objs::tree::EntryKind;
use rt_app::git::{GitCommit, GitIdentity};

use crate::branch::branch_ref;
use crate::commit::{describe, signature};
use crate::repo::Repo;
use crate::util::{Context, Result, Scope, fail};

/// One file of a [`Repo::commit_to_branch`]: its new bytes, or `None` to remove it. The path
/// is collection-relative, or repository-relative with a leading `/` (as in [`Repo::show`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFile {
    pub path: String,
    pub data: Option<Vec<u8>>,
}

impl TreeFile {
    pub fn write(path: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            data: Some(data.into()),
        }
    }

    pub fn remove(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            data: None,
        }
    }
}

impl Repo {
    /// Commits `files` onto the local branch `branch` and moves only that branch.
    ///
    /// - An existing branch gets the commit on top of its tip (`start` is ignored).
    /// - A missing branch is created at the commit `start` names (any revision, e.g. `HEAD`
    ///   or `main`), with the new commit on top.
    /// - The branch `HEAD` is on is refused: its working tree would no longer match.
    /// - Removing a file that is not there is fine; a commit that changes nothing is refused.
    ///
    /// The identity comes from the Git configuration, else `fallback`.
    pub fn commit_to_branch(
        &self,
        branch: &str,
        start: &str,
        files: &[TreeFile],
        message: &str,
        fallback: Option<&GitIdentity>,
    ) -> Result<GitCommit> {
        let message = message.trim();
        if message.is_empty() {
            return Err(fail("the commit message is empty"));
        }
        if files.is_empty() {
            return Err(fail("no files to commit"));
        }
        let full = branch_ref(branch)?;
        if self.head_branch()?.as_deref() == Some(branch) {
            return Err(fail(format!(
                "{branch} is checked out; commit to it from the working tree"
            )));
        }
        let tip = self.ref_commit(full.as_bstr().to_str_lossy().as_ref())?;
        let parent = match tip {
            Some(tip) => tip,
            None => self.resolve_rev(start)?,
        };
        let base = self.commit_tree(parent)?;
        let identity = self.identity_or(fallback)?;

        let mut editor = self.repo.edit_tree(base).ctx("cannot read a tree")?;
        let root = Scope::whole();
        for file in files {
            let path = match file.path.strip_prefix('/') {
                Some(rest) => root.abs(rest)?,
                None => self.scope.abs(&file.path)?,
            };
            match &file.data {
                Some(bytes) => {
                    let id: ObjectId = self
                        .repo
                        .write_blob(bytes)
                        .ctx("cannot write a file")?
                        .detach();
                    editor
                        .upsert(path.as_bstr(), EntryKind::Blob, id)
                        .ctx("cannot build the tree")?;
                }
                None => {
                    editor.remove(path.as_bstr()).ctx("cannot build the tree")?;
                }
            }
        }
        let tree = editor.write().ctx("cannot write the tree")?.detach();
        if tree == base {
            return Err(fail("nothing to commit: the files are unchanged"));
        }
        let signature = signature(&identity);
        // A missing branch is created; an existing one must still be at `tip`.
        let id = self.commit_on(
            &full.as_bstr().to_str_lossy(),
            &signature,
            &signature,
            message,
            tree,
            vec![parent],
        )?;
        describe(&self.repo, id)
    }
}

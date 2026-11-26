// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The index as building blocks (wave 15, strand F): diff each side, stage or unstage whole
//! files or chosen lines, discard chosen lines from the working tree, commit what is staged.
//!
//! The app's own commit form keeps "commit without staging" (wave 4): `GitApi.commit` takes
//! paths and keeps the index in step for them. These calls are generic Git for a staging view an
//! extension provides (`AppExtension.staging`); with none, nothing in the app calls them.
//!
//! Line selection works on the diff the same call reads: [`GitIndexDiff`] carries the blob ids
//! of both sides, and a [`GitPatch`] names them back, so a stale selection (the file
//! changed meanwhile) is refused instead of applied to the wrong lines. Paths are
//! repository-relative with a leading `/`.

use serde::{Deserialize, Serialize};

use super::GitDiff;

/// Which pair of versions a diff compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitPatchSide {
    /// Index → working tree (what `stage` would add).
    Unstaged,
    /// `HEAD` → index (what `unstage` would take back).
    Staged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitIndexDiff {
    pub side: GitPatchSide,
    pub diff: GitDiff,
    /// Blob id of the old side (`""` if the file does not exist there).
    pub old: String,
    /// Blob id of the new side (`""` if the file does not exist there; for the working tree the
    /// id the content would get).
    pub new: String,
}

/// Lines of one hunk of a [`GitIndexDiff`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHunkSelection {
    /// Index into `diff.hunks`.
    pub hunk: u32,
    /// Indexes into that hunk's `lines` (only added and removed lines count; context lines are
    /// ignored); `None`: the whole hunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<u32>>,
}

/// Apply the chosen part of a diff: stage it (side `unstaged`), unstage it (side `staged`) or,
/// for `discardPatch`, take it out of the working tree (side `unstaged`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPatch {
    pub path: String,
    pub side: GitPatchSide,
    /// [`GitIndexDiff::old`] and [`GitIndexDiff::new`] of the diff the selection was made on;
    /// refused with kind `git` when either changed since.
    pub old: String,
    pub new: String,
    pub hunks: Vec<GitHunkSelection>,
}

/// Commit what the index holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStagedCommit {
    pub message: String,
}

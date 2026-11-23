// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Commit-level building blocks (wave 15, strand D): read a commit in full, replay one commit's
//! change onto another in memory, write trees and commits as objects. Nothing here moves a ref,
//! touches the working tree or the index: objects that end up unreferenced are harmless and go
//! with Git's garbage collection.
//!
//! What builds on them moves refs only through [`super::actions::GitAction::Rewrite`], which
//! plans first (plain words, refused for published commits), writes a backup ref and an undo
//! entry, and updates the working tree like every other action. So a history editor (reorder,
//! squash, reword, drop, split) can live in an extension and runs the same in every shell; the
//! core only offers generic Git.
//!
//! Paths are repository-relative with a leading `/`.

use serde::{Deserialize, Serialize};

use super::GitConflict;

/// Author or committer of a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSignature {
    pub name: String,
    pub email: String,
    /// Unix seconds.
    pub time: i64,
    /// Time zone offset in minutes east of UTC (`+0100` → 60).
    #[serde(default)]
    pub offset: i32,
}

/// A commit in full (`GitApi.commitInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitDetails {
    pub oid: String,
    /// Its tree (full id).
    pub tree: String,
    /// First parent first.
    pub parents: Vec<String>,
    /// The whole message, as stored (trailing newline included).
    pub message: String,
    pub author: GitSignature,
    pub committer: GitSignature,
    /// It carries a signature (`gpgsig` header); whether it is valid says `GitApi.verify`.
    #[serde(default)]
    pub signed: bool,
}

/// Replay the change `commit` made (against its parent, or `mainline` for a merge) onto `onto`,
/// in memory, like `git cherry-pick` (or, with `revert`, its reverse) without a working tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPickRequest {
    /// A commit (rev) whose tree is the base of the result.
    pub onto: String,
    pub commit: String,
    /// Apply the reverse change (`git revert`).
    #[serde(default)]
    pub revert: bool,
    /// For a merge commit: the parent (1-based) whose side counts as the change's base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainline: Option<u32>,
}

/// The replayed tree. Collection files merge semantically (wave 12), every other file line by
/// line, as merge, cherry-pick and revert do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPickResult {
    /// The merged tree (full id). Conflicted paths hold `onto`'s version in it; write the
    /// resolved content over them with `GitApi.writeTree`.
    pub tree: String,
    /// No conflicts: `tree` is the result.
    pub clean: bool,
    /// Text conflicts: base, ours (`onto`) and theirs (the change), `None` where a side lacks
    /// the file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<GitConflict>,
    /// Paths with conflicts that are not text (binary, file vs. folder, submodule): take one
    /// side with `writeTree`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_conflicts: Vec<String>,
}

/// New content of one path in [`GitTreeWrite`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum GitTreeContent {
    Text {
        text: String,
    },
    Base64 {
        data: String,
    },
    /// The path as it is in another tree or commit (`rev`), e.g. "take theirs" for a binary
    /// conflict. Missing there: the path is removed.
    From {
        rev: String,
    },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitTreeFile {
    /// Repository-relative, leading `/`.
    pub path: String,
    #[serde(flatten)]
    pub content: GitTreeContent,
    /// Executable file mode (`100755`); else the path keeps its mode, new files get `100644`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<bool>,
}

/// A tree: `base` (a tree or commit rev) with `files` written or removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitTreeWrite {
    pub base: String,
    pub files: Vec<GitTreeFile>,
}

/// A commit object. Nothing points at it until an action moves a ref to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitWrite {
    /// Tree id.
    pub tree: String,
    /// Parent commits (revs), first parent first; empty for a root commit.
    pub parents: Vec<String>,
    pub message: String,
    /// `None`: the app's identity, now (as a new commit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<GitSignature>,
    /// `None`: the app's identity, now. Rewritten commits keep their author and get a new
    /// committer, as `git rebase` does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer: Option<GitSignature>,
}

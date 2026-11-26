// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Inspecting the history of any file (wave 15, strand C): line blame, search across commits
//! (message, author, path, content) and the reflog, the base of "undo" for anything Git did,
//! also outside the app.
//!
//! The history of one file is `GitApi.fileLog` (wave 12) with a repository-relative path (leading
//! `/`); it follows renames for every file. Paths here are
//! repository-relative with a leading `/`.

use serde::{Deserialize, Serialize};

use super::GitCommit;
use super::graph::GitGraphCommit;

/// Consecutive lines that came from the same commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBlameHunk {
    /// 1-based first line in the blamed version.
    pub start: u32,
    pub lines: u32,
    pub commit: GitCommit,
    /// The file's path in that commit (differs after a rename).
    pub path: String,
    /// 1-based first line in that commit's version.
    pub orig_start: u32,
}

/// Who last changed each line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBlame {
    pub path: String,
    /// The commit blamed (full id).
    pub rev: String,
    /// Lines of the file at `rev`.
    pub lines: u32,
    /// In line order, covering every line.
    pub hunks: Vec<GitBlameHunk>,
    /// Binary files have no lines: `hunks` is empty.
    #[serde(default)]
    pub binary: bool,
}

/// What to look for in the history. Every given field must match (and); text matching is
/// case-insensitive substring matching.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitHistoryQuery {
    /// In the commit message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// In the author's name or email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The commit touched a path that starts with this (repository-relative, leading `/`; a
    /// folder matches everything below it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The commit added or removed a line containing this (like `git log -S`, but
    /// case-insensitive): the slow part, the shells stop at [`HISTORY_SCAN_LIMIT`] commits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Author time, Unix seconds, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    /// Start from these refs (full names); `None`: all refs and `HEAD`, as the graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refs: Option<Vec<String>>,
    /// Hits to skip (paging) and at most this many.
    #[serde(default)]
    pub skip: u32,
    pub limit: u32,
}

/// Commits a content search reads at most per call; [`GitHistoryPage::more`] continues.
pub const HISTORY_SCAN_LIMIT: u32 = 20_000;

/// What part of a commit matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitHistoryField {
    Message,
    Author,
    Path,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHistoryHit {
    pub commit: GitGraphCommit,
    pub matched: Vec<GitHistoryField>,
    /// Paths that matched `path` or `content` (repository-relative, leading `/`), at most a few.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitHistoryPage {
    /// Newest first, in the graph's order.
    pub hits: Vec<GitHistoryHit>,
    /// More hits may follow (ask with `skip + hits.len()`).
    #[serde(default)]
    pub more: bool,
    /// Commits read for this page.
    #[serde(default)]
    pub scanned: u32,
}

/// One entry of a ref's reflog, newest first: where the ref pointed before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitReflogEntry {
    /// Full ref name (`HEAD`, `refs/heads/main`).
    pub name: String,
    /// `0` is the newest (`HEAD@{0}`).
    pub index: u32,
    /// Full ids; `None` before the ref existed.
    pub old: Option<String>,
    pub new: String,
    /// Git's reflog message, e.g. `commit: Add login`, `checkout: moving from a to b`.
    pub message: String,
    /// Unix seconds.
    pub time: i64,
    /// The commit `new` names, for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<GitCommit>,
}

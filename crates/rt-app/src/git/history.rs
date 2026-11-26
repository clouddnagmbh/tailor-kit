// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Inspecting the history of any file (wave 15, strand C): line blame, search across commits
//! (message, author, path, content) and the reflog, the base of "undo" for anything Git did,
//! also outside the app.
//!
//! The history of one file is `GitApi.fileLog` (wave 12) with a repository-relative path (leading
//! `/`); it follows renames for every file, not only collection files. Paths here are
//! repository-relative with a leading `/`.
//!
//! The types live in the Tailor Kit (`tk_git_types::history`); this module keeps the old paths.

pub use tk_git_types::history::*;

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
//!
//! The types live in the Tailor Kit (`tk_git_types::plumbing`, where [`GitPickRequest`] is called
//! `GitPick`); this module keeps the old paths.

pub use tk_git_types::plumbing::GitPick as GitPickRequest;
pub use tk_git_types::plumbing::*;

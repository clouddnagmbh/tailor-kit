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
//! of both sides, and a [`GitPatchRequest`] names them back, so a stale selection (the file
//! changed meanwhile) is refused instead of applied to the wrong lines. Paths are
//! repository-relative with a leading `/`.
//!
//! The types live in the Tailor Kit (`tk_git_types::index`, where [`GitPatchRequest`] is called
//! `GitPatch`); this module keeps the old paths.

pub use tk_git_types::index::GitPatch as GitPatchRequest;
pub use tk_git_types::index::*;

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The working copy and the files of any repository (wave 15, strand B): the status of the whole
//! working tree, a file tree per folder, and file contents for the viewers (text with word diff,
//! images side by side, binary files as metadata).
//!
//! Generic: nothing assumes a collection. Paths are repository-relative with a leading `/`. A
//! file that is a collection file names its collection ([`GitWorkingFile::collection`], as
//! [`super::graph::GitChangedFile::collection`]) so the UI can offer the semantic diff.
//!
//! The default model stays "commit without staging" (wave 4): [`GitWorkingFile::state`] compares
//! the working tree with `HEAD`. [`GitWorkingFile::staged`] and [`GitWorkingFile::unstaged`] say
//! what the index holds, for a staging view an extension may add ([`super::index`]).
//!
//! Budget: the status of a working tree with 50 000 tracked files under 300 ms natively
//! (`perf/budgets.toml`, `git_status_large`).

//!
//! The types live in the Tailor Kit (`tk_git_types::files`); RequestTailor adds the collection
//! of each file ([`InCollection`]).

pub use tk_git_types::files::*;

use super::InCollection;
use super::graph::{CollectionAt, collection_of};

/// One changed file of the working copy, with the collection of the set it belongs to
/// (`CollectionEntry::id`), if any.
pub type GitWorkingFile = tk_git_types::files::GitWorkingFile<InCollection>;

/// The working copy of the active repository (`repository`: [`super::repos::RepositoryEntry::id`]).
pub type GitWorkingCopy = tk_git_types::files::GitWorkingCopy<InCollection>;

/// The working copy the Git core reads, with the collection of every file that belongs to one of
/// `collections`.
pub fn in_collections(
    copy: tk_git_types::files::GitWorkingCopy,
    collections: &[CollectionAt],
) -> GitWorkingCopy {
    copy.with_extra(|f| InCollection {
        collection: collection_of(&f.path, collections).map(str::to_owned),
    })
}

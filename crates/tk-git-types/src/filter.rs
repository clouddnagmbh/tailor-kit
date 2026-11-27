// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Content filters (seam N11): what `filter=<name>` in `.gitattributes` does to a file between
//! the working tree and the object store, in place of Git's `filter.<name>.clean`/`.smudge`
//! programs (GitTailor wave 3, strand E: Git LFS without `git-lfs`).
//!
//! The Git core asks the product's [`ContentFilter`] for every path whose `filter` attribute it
//! [handles](ContentFilter::handles):
//!
//! - **clean** (working tree → Git) when it hashes or stores a working file: status, diff,
//!   commit, staging, snapshots and the working-file backups of undo;
//! - **smudge** (Git → working tree) when it writes a file: checkout, switch, clone, pull,
//!   merge, restore, undo, and when it reads a blob at a revision for the viewers.
//!
//! Like Git, clean runs before the end-of-line conversion and smudge after it. Git's own
//! driver configuration for a handled name (`filter.<name>.process` etc.) is not run. A name the
//! filter does not handle, and every path without a `filter` attribute, goes through the core
//! exactly as without a filter; without a filter nothing changes at all.

use std::path::Path;

use crate::GitResult;

/// The file a filter works on.
#[derive(Debug, Clone, Copy)]
pub struct FilterFile<'a> {
    /// Canonical root of the working tree.
    pub workdir: &'a Path,
    /// The Git directory every working tree of the repository shares (`objects`, `config`).
    pub common_dir: &'a Path,
    /// The value of the `filter` attribute (`lfs`).
    pub name: &'a str,
    /// Repository-relative path, `/`-separated, no leading `/`.
    pub path: &'a str,
}

/// Converts the content of the files whose `filter` attribute it handles.
pub trait ContentFilter: Send + Sync {
    /// Whether `filter=<name>` is this filter's.
    fn handles(&self, name: &str) -> bool;

    /// The working-tree bytes of `file` → what Git stores. An error stops the operation that
    /// reads the file (nothing is stored).
    fn clean(&self, file: &FilterFile<'_>, data: Vec<u8>) -> GitResult<Vec<u8>>;

    /// What Git stores → the working-tree bytes. A filter that cannot produce the content now
    /// (an object that is not available offline) returns `data` unchanged rather than failing:
    /// an error stops a checkout half way.
    fn smudge(&self, file: &FilterFile<'_>, data: Vec<u8>) -> GitResult<Vec<u8>>;

    /// Called once before a checkout smudges `files` (with what Git stores for each), so the
    /// filter can fetch what it needs in one go. Default: nothing.
    fn prepare_smudge(&self, files: &[(FilterFile<'_>, &[u8])]) -> GitResult<()> {
        let _ = files;
        Ok(())
    }
}

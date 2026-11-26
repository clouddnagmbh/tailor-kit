// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Seam N2: what checks the files of a commit before it is written. The Git core hands every
//! file a commit would add or change to the [`CommitScanner`] of the repository; a refusal
//! stops the commit with [`crate::GitError::SecretsFound`] before any ref moves. Without a
//! scanner, nothing is checked.

/// Checks the files of a commit. Implementations must be pure: same files, same answer.
pub trait CommitScanner: Send + Sync {
    /// `files` are the added and changed files of the commit, as (path as the caller names
    /// it, content as committed); deleted files are not listed. `None` lets the commit
    /// through; `Some(message)` refuses it. The message is shown to the user and must not
    /// contain the secret it found.
    fn refusal(&self, files: &[(&str, &[u8])]) -> Option<String>;
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Errors, scope-relative paths and flattened trees.

use std::collections::BTreeMap;

use gix::ObjectId;
use gix::bstr::{BStr, BString, ByteSlice};
use gix::index::entry::Mode;
pub use tk_git_types::GitError;

/// Result of the Git core (seam N7): `GitError`, which a product turns into its own error.
pub type Result<T, E = GitError> = std::result::Result<T, E>;

/// A refused or failed Git operation.
pub fn fail(message: impl Into<String>) -> GitError {
    GitError::Failed(message.into())
}

/// Turns any error into `GitError::Failed` with what was being done.
pub trait Context<T> {
    fn ctx(self, what: &str) -> Result<T>;
}

impl<T, E: std::fmt::Display> Context<T> for std::result::Result<T, E> {
    fn ctx(self, what: &str) -> Result<T> {
        self.map_err(|e| GitError::Failed(format!("{what}: {e}")))
    }
}

/// `std::fs::canonicalize` without Windows' verbatim prefix (`\\?\C:\x` → `C:\x`), so paths
/// compare, print and go into Git URLs the usual way.
pub fn canonical(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let path = std::fs::canonicalize(path)?;
    if cfg!(windows)
        && let Some(s) = path.to_str()
        && let Some(rest) = s.strip_prefix(r"\\?\")
        && rest.as_bytes().get(1) == Some(&b':')
    {
        return Ok(std::path::PathBuf::from(rest));
    }
    Ok(path)
}

/// The part of the repository a `Repo` works on (seam N3: `tk_git_types::RepoScope`), with
/// repository paths as Git bytes.
#[derive(Debug, Clone)]
pub struct Scope(tk_git_types::RepoScope);

impl Scope {
    /// The whole repository.
    pub fn whole() -> Self {
        Self(tk_git_types::RepoScope::whole())
    }

    /// The folder `prefix` (repository-relative); refused if a path could not name it.
    pub fn folder(prefix: &str) -> Result<Self> {
        Ok(Self(tk_git_types::RepoScope::folder(prefix)?))
    }

    pub fn scope(&self) -> &tk_git_types::RepoScope {
        &self.0
    }

    pub fn prefix(&self) -> &BStr {
        self.0.prefix().as_bytes().as_bstr()
    }

    /// Whether the repository path `path` belongs to the scope.
    pub fn contains(&self, path: &BStr) -> bool {
        self.0.contains(path)
    }

    /// Scope-relative form of a repository path, `None` outside the scope (or not UTF-8).
    pub fn rel(&self, path: &BStr) -> Option<String> {
        self.0.rel(path)
    }

    /// Repository path of the scope-relative `path`; rejects anything that could leave it.
    pub fn abs(&self, path: &str) -> Result<BString> {
        Ok(self.0.abs(path)?.into())
    }
}

/// One file of a tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blob {
    pub mode: Mode,
    pub id: ObjectId,
}

/// All files of a tree (no folders), by repository path.
pub type Flat = BTreeMap<BString, Blob>;

/// Every file of `tree` (submodule entries included, as `Mode::COMMIT`).
pub fn flatten(repo: &gix::Repository, tree: ObjectId) -> Result<Flat> {
    let index = repo.index_from_tree(&tree).ctx("cannot read a tree")?;
    Ok(index
        .entries()
        .iter()
        .map(|e| {
            (
                e.path(&index).to_owned(),
                Blob {
                    mode: e.mode,
                    id: e.id,
                },
            )
        })
        .collect())
}

/// Text of a blob for display: `None` if it is missing; lossy for non-UTF-8 bytes.
pub fn blob_text(repo: &gix::Repository, id: ObjectId) -> Result<String> {
    let blob = repo
        .find_blob(id)
        .ctx("cannot read a file from the repository")?;
    Ok(String::from_utf8_lossy(&blob.data).into_owned())
}

/// Bytes of a blob.
pub fn blob_bytes(repo: &gix::Repository, id: ObjectId) -> Result<Vec<u8>> {
    let mut blob = repo
        .find_blob(id)
        .ctx("cannot read a file from the repository")?;
    Ok(blob.take_data())
}

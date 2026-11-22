// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Errors, collection-relative paths and flattened trees.

use std::collections::BTreeMap;

use gix::ObjectId;
use gix::bstr::{BStr, BString, ByteSlice};
use gix::index::entry::Mode;
use rt_app::AppError;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

/// A refused or failed Git operation.
pub fn fail(message: impl Into<String>) -> AppError {
    AppError::Git(message.into())
}

/// Turns any error into `AppError::Git` with what was being done.
pub trait Context<T> {
    fn ctx(self, what: &str) -> Result<T>;
}

impl<T, E: std::fmt::Display> Context<T> for std::result::Result<T, E> {
    fn ctx(self, what: &str) -> Result<T> {
        self.map_err(|e| AppError::Git(format!("{what}: {e}")))
    }
}

/// The collection's place in the repository: repository-relative paths with this prefix
/// belong to it.
#[derive(Debug, Clone)]
pub struct Scope {
    /// `""` (collection is the work tree) or `a/b` (no trailing slash).
    prefix: BString,
}

impl Scope {
    pub fn new(prefix: BString) -> Self {
        Self { prefix }
    }

    pub fn prefix(&self) -> &BStr {
        self.prefix.as_bstr()
    }

    /// Whether the repository path `path` belongs to the collection.
    pub fn contains(&self, path: &BStr) -> bool {
        self.rel_bytes(path).is_some()
    }

    fn rel_bytes<'a>(&self, path: &'a BStr) -> Option<&'a BStr> {
        if self.prefix.is_empty() {
            return Some(path);
        }
        let rest = path.strip_prefix(self.prefix.as_slice())?;
        let rest = rest.strip_prefix(b"/")?;
        (!rest.is_empty()).then(|| rest.as_bstr())
    }

    /// Collection-relative form of a repository path, `None` outside the collection (or not
    /// UTF-8, which the collection cannot hold either).
    pub fn rel(&self, path: &BStr) -> Option<String> {
        self.rel_bytes(path)
            .and_then(|p| p.to_str().ok())
            .map(str::to_owned)
    }

    /// Repository path of the collection-relative `path`; rejects anything that could leave
    /// the collection.
    pub fn abs(&self, path: &str) -> Result<BString> {
        let bad = || fail(format!("invalid path: {path:?}"));
        if path.is_empty() || path.contains('\\') || path.contains('\0') {
            return Err(bad());
        }
        for part in path.split('/') {
            if part.is_empty() || part == "." || part == ".." || part.eq_ignore_ascii_case(".git") {
                return Err(bad());
            }
        }
        Ok(if self.prefix.is_empty() {
            path.into()
        } else {
            let mut p = self.prefix.clone();
            p.push(b'/');
            p.extend_from_slice(path.as_bytes());
            p
        })
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
    let index = repo
        .index_from_tree(&tree)
        .ctx("cannot read a tree")?;
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
    let blob = repo.find_blob(id).ctx("cannot read a file from the repository")?;
    Ok(String::from_utf8_lossy(&blob.data).into_owned())
}

/// Bytes of a blob.
pub fn blob_bytes(repo: &gix::Repository, id: ObjectId) -> Result<Vec<u8>> {
    let mut blob = repo.find_blob(id).ctx("cannot read a file from the repository")?;
    Ok(blob.take_data())
}

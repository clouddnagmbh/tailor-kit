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

use serde::{Deserialize, Serialize};

use super::{GitFileChange, GitFileState, repos::GitOperation};

/// What a path in the working tree or in a commit's tree is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitEntryKind {
    #[default]
    File,
    Dir,
    Symlink,
    /// A submodule (a commit in the tree, a checkout or an empty folder in the working tree).
    Submodule,
}

/// One changed file of the working copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitWorkingFile {
    /// Repository-relative, leading `/`.
    pub path: String,
    /// `HEAD` → working tree (what the default commit form shows).
    pub state: GitFileState,
    /// A rename detected between `HEAD` and the working tree: the old path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "is_file")]
    pub kind: GitEntryKind,
    /// `HEAD` → index: how the index differs from `HEAD` for this path, if it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged: Option<GitFileChange>,
    /// The working tree differs from the index for this path.
    #[serde(default)]
    pub unstaged: bool,
    /// The collection of the set this file belongs to (`CollectionEntry::id`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
}

fn is_file(k: &GitEntryKind) -> bool {
    *k == GitEntryKind::File
}

/// The working copy of the active repository.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitWorkingCopy {
    /// [`super::repos::RepositoryEntry::id`].
    pub repository: String,
    /// `None`: detached `HEAD` or no commit yet.
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The commit `HEAD` points to; `None` without commits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default)]
    pub detached: bool,
    /// A merge, cherry-pick, revert or rebase is half done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<GitOperation>,
    /// Changed, new, deleted and conflicted files, sorted by path. Ignored files are left out.
    pub files: Vec<GitWorkingFile>,
    /// More than [`WORKING_COPY_LIMIT`] changed files: the list stops there.
    #[serde(default)]
    pub truncated: bool,
}

/// At most this many files in [`GitWorkingCopy::files`] (a checkout gone wrong can change every
/// file; the UI shows the count and offers to discard or park them all).
pub const WORKING_COPY_LIMIT: usize = 10_000;

/// One entry of a folder in the file tree (`GitApi.fileTree`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitTreeEntry {
    /// Last path segment.
    pub name: String,
    /// Repository-relative, leading `/`.
    pub path: String,
    pub kind: GitEntryKind,
    /// Files: size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Working tree only: the file's state against `HEAD`, if it changed. Folders: whether
    /// anything below changed is the UI's business (from `workingCopy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<GitFileState>,
    /// Working tree only: matched by `.gitignore` (shown dimmed, never listed in changes).
    #[serde(default)]
    pub ignored: bool,
}

/// A file's content for the viewers: text up to [`BLOB_TEXT_LIMIT`], images up to
/// [`BLOB_IMAGE_LIMIT`] as base64, anything else as metadata only.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitBlob {
    /// Repository-relative, leading `/`.
    pub path: String,
    /// The commit it was read at; `None`: the working tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// The blob id (`None` in the working tree when the file is not in the index).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
    pub size: u64,
    /// Contains a NUL byte in the first 8 KiB or is not UTF-8.
    pub binary: bool,
    /// From the extension ([`media_type`]), e.g. `image/png`; `None` if unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Text files up to the limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Images (`media_type` `image/*`) up to the limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    /// Larger than the limit: neither `text` nor `base64`.
    #[serde(default)]
    pub truncated: bool,
    /// The content is a Git LFS pointer (`version https://git-lfs.github.com/spec/v1`), not the
    /// file itself.
    #[serde(default)]
    pub lfs_pointer: bool,
}

/// Text shown in viewers up to this size (bytes).
pub const BLOB_TEXT_LIMIT: u64 = 8 * 1024 * 1024;
/// Images shown up to this size (bytes).
pub const BLOB_IMAGE_LIMIT: u64 = 16 * 1024 * 1024;

/// The media type the viewers use for `path`, by extension (case-insensitive); `None` for
/// anything they show as text or metadata only.
pub fn media_type(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_types_by_extension() {
        assert_eq!(media_type("/img/Logo.PNG"), Some("image/png"));
        assert_eq!(media_type("/a.svg"), Some("image/svg+xml"));
        assert_eq!(media_type("/README.md"), None);
        assert_eq!(media_type("/Makefile"), None);
    }

    #[test]
    fn plain_files_omit_their_kind() {
        let f = GitWorkingFile {
            path: "/a".into(),
            state: GitFileState::Modified,
            from: None,
            kind: GitEntryKind::File,
            staged: None,
            unstaged: false,
            collection: None,
        };
        let json = serde_json::to_value(&f).unwrap();
        assert!(json.get("kind").is_none());
        assert_eq!(serde_json::from_value::<GitWorkingFile>(json).unwrap(), f);
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The working copy and the files of any repository (wave 15, strand B): the status of the whole
//! working tree, a file tree per folder, and file contents for the viewers (text with word diff,
//! images side by side, binary files as metadata).
//!
//! Generic: nothing assumes anything about the files. Paths are repository-relative with a
//! leading `/`. A product adds its own fields to every file through [`GitWorkingFile::extra`]
//! (as [`super::graph::GitChangedFile::extra`]), e.g. which of its documents a file belongs to,
//! so its UI can offer a diff on its content model; the Git core leaves them at their default.
//!
//! The default model stays "commit without staging" (wave 4): [`GitWorkingFile::state`] compares
//! the working tree with `HEAD`. [`GitWorkingFile::staged`] and [`GitWorkingFile::unstaged`] say
//! what the index holds, for a staging view an extension may add ([`super::index`]).
//!
//! Budget: the status of a working tree with 50 000 tracked files under 300 ms natively
//! (`perf/budgets.toml`, `git_status_large`).

use serde::{Deserialize, Serialize};

use super::{GitFileChange, GitFileState, NoExtra, repos::GitOperation};

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

/// One changed file of the working copy. `X`: the product's own fields ([`NoExtra`]: none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitWorkingFile<X = NoExtra> {
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
    /// The product's own fields, serialized after the others (flattened, last).
    #[serde(flatten)]
    pub extra: X,
}

impl<X> GitWorkingFile<X> {
    /// The same file with the product's fields `extra` in place of these.
    pub fn with_extra<Y>(self, extra: Y) -> GitWorkingFile<Y> {
        GitWorkingFile {
            path: self.path,
            state: self.state,
            from: self.from,
            kind: self.kind,
            staged: self.staged,
            unstaged: self.unstaged,
            extra,
        }
    }
}

fn is_file(k: &GitEntryKind) -> bool {
    *k == GitEntryKind::File
}

/// The working copy of the active repository. `X`: the product's own fields of every file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitWorkingCopy<X = NoExtra> {
    /// The repository: the core fills in its work tree path; a shell may put its own id here.
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
    pub files: Vec<GitWorkingFile<X>>,
    /// More than [`WORKING_COPY_LIMIT`] changed files: the list stops there.
    #[serde(default)]
    pub truncated: bool,
}

impl<X> GitWorkingCopy<X> {
    /// The same working copy with the product's fields of each file from `extra`.
    pub fn with_extra<Y>(
        self,
        mut extra: impl FnMut(&GitWorkingFile<X>) -> Y,
    ) -> GitWorkingCopy<Y> {
        GitWorkingCopy {
            repository: self.repository,
            branch: self.branch,
            upstream: self.upstream,
            ahead: self.ahead,
            behind: self.behind,
            head: self.head,
            detached: self.detached,
            operation: self.operation,
            files: self
                .files
                .into_iter()
                .map(|f| {
                    let y = extra(&f);
                    f.with_extra(y)
                })
                .collect(),
            truncated: self.truncated,
        }
    }
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
            extra: NoExtra {},
        };
        let json = serde_json::to_value(&f).unwrap();
        assert!(json.get("kind").is_none());
        assert_eq!(serde_json::from_value::<GitWorkingFile>(json).unwrap(), f);
    }

    /// A product's fields: flattened after the core's, omitted like any optional field.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
    struct Owner {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<String>,
    }

    #[test]
    fn extra_fields_come_last_and_round_trip() {
        let f = GitWorkingFile {
            path: "/a".into(),
            state: GitFileState::Untracked,
            from: None,
            kind: GitEntryKind::File,
            staged: None,
            unstaged: true,
            extra: Owner {
                owner: Some("x".into()),
            },
        };
        assert_eq!(
            serde_json::to_string(&f).unwrap(),
            r#"{"path":"/a","state":"untracked","unstaged":true,"owner":"x"}"#
        );
        let back: GitWorkingFile<Owner> =
            serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(back, f);
        let none = f.clone().with_extra(Owner::default());
        assert_eq!(
            serde_json::to_string(&none).unwrap(),
            r#"{"path":"/a","state":"untracked","unstaged":true}"#
        );
        let plain: GitWorkingFile =
            serde_json::from_str(r#"{"path":"/a","state":"untracked","owner":"x"}"#).unwrap();
        assert_eq!(plain.extra, NoExtra {});
    }
}

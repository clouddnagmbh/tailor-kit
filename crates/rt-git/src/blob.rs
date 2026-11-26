// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The content of one file for the viewers (`GitApi.blob`, wave 15 strand B): at a revision or
//! in the working tree. Text up to [`BLOB_TEXT_LIMIT`], images up to [`BLOB_IMAGE_LIMIT`] as
//! base64, anything else (binary files, Git LFS pointers' targets, big files) as metadata.

use gix::bstr::{BString, ByteSlice};
use gix::index::entry::Mode;
use rt_app::git::files::{BLOB_IMAGE_LIMIT, BLOB_TEXT_LIMIT, GitBlob, media_type};

use crate::repo::Repo;
use crate::util::{Context, Result, Scope, fail};

/// Looked at to decide "binary" (the same window Git uses).
const SNIFF: usize = 8 * 1024;
/// An LFS pointer is a few lines; anything bigger is the file itself.
const LFS_POINTER_MAX: u64 = 1024;
const LFS_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1";

/// A repository path from an API path (`/a/b` or `a/b`); rejects anything odd.
pub(crate) fn repo_path(path: &str) -> Result<BString> {
    Scope::whole().abs(path.strip_prefix('/').unwrap_or(path))
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(char::from(ALPHABET[(n >> 18) as usize & 63]));
        out.push(char::from(ALPHABET[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(ALPHABET[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(ALPHABET[n as usize & 63])
        } else {
            '='
        });
    }
    out
}

impl Repo {
    /// `path` (repository-relative, leading `/`) at `rev`, or in the working tree for `None`.
    pub fn blob(&self, rev: Option<&str>, path: &str) -> Result<GitBlob> {
        let repo_path = repo_path(path)?;
        let api = format!("/{}", repo_path.to_str_lossy());
        let mut blob = GitBlob {
            path: api.clone(),
            rev: None,
            ..GitBlob::default()
        };
        let media = media_type(&api);
        blob.media_type = media.map(str::to_owned);
        let image = media.is_some_and(|m| m.starts_with("image/"));
        let read_limit = if image {
            BLOB_IMAGE_LIMIT.max(BLOB_TEXT_LIMIT)
        } else {
            BLOB_TEXT_LIMIT
        };

        // The bytes (up to the limit), the full size, and the blob id if known.
        let (bytes, size): (Vec<u8>, u64) = match rev {
            Some(rev) => {
                let commit = self.resolve_rev(rev)?;
                blob.rev = Some(commit.to_string());
                let Some((mode, id)) = self.entry_at(commit, &repo_path)? else {
                    return Err(fail(format!("{api} does not exist at {rev}")));
                };
                if mode == Mode::COMMIT {
                    return Err(fail(format!("{api} is a submodule")));
                }
                if mode == Mode::DIR {
                    return Err(fail(format!("{api} is a folder")));
                }
                blob.oid = Some(id.to_string());
                let size = self
                    .repo
                    .find_header(id)
                    .ctx("cannot read a file from the repository")?
                    .size();
                if size <= read_limit {
                    let data = self
                        .repo
                        .find_blob(id)
                        .ctx("cannot read a file from the repository")?
                        .take_data();
                    (data, size)
                } else {
                    // Only enough to tell binary from text.
                    let mut data = self
                        .repo
                        .find_blob(id)
                        .ctx("cannot read a file from the repository")?
                        .take_data();
                    data.truncate(SNIFF);
                    (data, size)
                }
            }
            None => {
                let file = crate::worktree::fs_path(&self.workdir, repo_path.as_bstr())?;
                let md = std::fs::symlink_metadata(&file)
                    .map_err(|_| fail(format!("{api} does not exist in the working tree")))?;
                if md.is_dir() {
                    return Err(fail(format!("{api} is a folder")));
                }
                if md.is_symlink() {
                    let target = std::fs::read_link(&file).ctx("cannot read a symbolic link")?;
                    let data = gix::path::into_bstr(target)
                        .ctx("invalid symbolic link")?
                        .into_owned()
                        .to_vec();
                    let size = data.len() as u64;
                    (data, size)
                } else if md.len() <= read_limit {
                    let data = std::fs::read(&file).ctx("cannot read a file")?;
                    let size = data.len() as u64;
                    blob.oid = gix::objs::compute_hash(
                        self.repo.object_hash(),
                        gix::objs::Kind::Blob,
                        &data,
                    )
                    .ok()
                    .map(|id| id.to_string());
                    (data, size)
                } else {
                    use std::io::Read as _;
                    let mut data = Vec::new();
                    std::fs::File::open(&file)
                        .and_then(|f| f.take(SNIFF as u64).read_to_end(&mut data))
                        .ctx("cannot read a file")?;
                    (data, md.len())
                }
            }
        };
        blob.size = size;
        let complete = bytes.len() as u64 == size;

        let sniffed = &bytes[..bytes.len().min(SNIFF)];
        let raster = image && media != Some("image/svg+xml");
        let text_ok = !sniffed.contains(&0) && (!complete || std::str::from_utf8(&bytes).is_ok());
        blob.binary = raster || !text_ok;

        if size <= LFS_POINTER_MAX && bytes.starts_with(LFS_PREFIX) {
            blob.lfs_pointer = true;
        }
        if !blob.binary && size <= BLOB_TEXT_LIMIT && complete {
            blob.text = Some(String::from_utf8_lossy(&bytes).into_owned());
        }
        if image && size <= BLOB_IMAGE_LIMIT && complete {
            blob.base64 = Some(base64(&bytes));
        }
        blob.truncated = !complete;
        Ok(blob)
    }
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_standard_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}

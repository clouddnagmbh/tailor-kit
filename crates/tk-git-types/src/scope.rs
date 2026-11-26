// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Seam N3: which part of a repository a program works on: the whole repository, or one
//! folder of it (a product's document folder). Paths a program sees are relative
//! to its [`RepoScope`]; [`ScopeFiles`] says which files inside it count (without one, all).

use std::collections::BTreeMap;

use crate::{GitError, GitResult};

/// The whole repository, or one folder of it (repository-relative, `/`-separated, no
/// trailing `/`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoScope {
    /// `""` for the whole repository.
    prefix: String,
}

impl RepoScope {
    /// The whole repository.
    pub fn whole() -> Self {
        Self::default()
    }

    /// The folder `prefix` (repository-relative); `""` is the whole repository. Refuses
    /// anything [`RepoScope::abs`] refuses.
    pub fn folder(prefix: &str) -> GitResult<Self> {
        let prefix = prefix.trim_end_matches('/');
        if prefix.is_empty() {
            return Ok(Self::whole());
        }
        Ok(Self {
            prefix: Self::whole().abs(prefix)?,
        })
    }

    /// `""` for the whole repository, else the folder.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn is_whole(&self) -> bool {
        self.prefix.is_empty()
    }

    fn rel_bytes<'a>(&self, path: &'a [u8]) -> Option<&'a [u8]> {
        if self.prefix.is_empty() {
            return Some(path);
        }
        let rest = path.strip_prefix(self.prefix.as_bytes())?;
        let rest = rest.strip_prefix(b"/")?;
        (!rest.is_empty()).then_some(rest)
    }

    /// Whether the repository path `path` lies inside the scope.
    pub fn contains(&self, path: &[u8]) -> bool {
        self.rel_bytes(path).is_some()
    }

    /// Scope-relative form of a repository path; `None` outside the scope or not UTF-8.
    pub fn rel(&self, path: &[u8]) -> Option<String> {
        self.rel_bytes(path)
            .and_then(|p| std::str::from_utf8(p).ok())
            .map(str::to_owned)
    }

    /// Repository path of the scope-relative `path`. Refuses anything that could leave the
    /// scope or name Git's own folder: empty parts, `.`, `..`, `.git`, `\` and NUL.
    pub fn abs(&self, path: &str) -> GitResult<String> {
        let bad = || GitError::failed(format!("invalid path: {path:?}"));
        if path.is_empty() || path.contains('\\') || path.contains('\0') {
            return Err(bad());
        }
        for part in path.split('/') {
            if part.is_empty() || part == "." || part == ".." || part.eq_ignore_ascii_case(".git") {
                return Err(bad());
            }
        }
        Ok(if self.prefix.is_empty() {
            path.to_owned()
        } else {
            format!("{}/{path}", self.prefix)
        })
    }
}

/// Which files inside the scope count when a program lists the files of a revision or
/// looks for the source of a rename. Without one, every regular file counts.
pub trait ScopeFiles: Send + Sync {
    /// Whether the scope-relative `rel` is one of the program's files.
    fn includes(&self, rel: &str) -> bool;

    /// Removes files that belong to a nested unit of their own (a nested document folder)
    /// from a revision's files (scope-relative path → text). The default keeps all.
    fn drop_nested(&self, files: &mut BTreeMap<String, String>) {
        let _ = files;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_and_folder_map_paths_both_ways() {
        let whole = RepoScope::whole();
        assert!(whole.is_whole());
        assert_eq!(whole.rel(b"a/b.yml").as_deref(), Some("a/b.yml"));
        assert_eq!(whole.abs("a/b.yml").unwrap(), "a/b.yml");

        let users = RepoScope::folder("api/users/").unwrap();
        assert_eq!(users.prefix(), "api/users");
        assert_eq!(users.rel(b"api/users/get.yml").as_deref(), Some("get.yml"));
        assert_eq!(users.abs("get.yml").unwrap(), "api/users/get.yml");
        assert!(users.contains(b"api/users/x/y.yml"));
        assert!(!users.contains(b"api/users"));
        assert!(!users.contains(b"api/usersx/get.yml"));
        assert!(users.rel(b"api/other.yml").is_none());
        assert_eq!(RepoScope::folder("").unwrap(), RepoScope::whole());
    }

    #[test]
    fn paths_that_leave_the_scope_are_refused() {
        let users = RepoScope::folder("users").unwrap();
        for bad in [
            "",
            "../x",
            "a/../b",
            "./a",
            "a//b",
            ".git/config",
            "a/.GIT",
            "a\\b",
            "a\0",
        ] {
            assert!(users.abs(bad).is_err(), "{bad:?}");
        }
        assert!(RepoScope::folder("../up").is_err());
        assert!(RepoScope::folder(".git").is_err());
    }
}

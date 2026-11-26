// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Seam N8: where a program keeps its own data inside a repository. Every product names its
//! namespace once; the Git core builds every ref and file name of its own from it.
//!
//! - refs under `refs/<name>/…` (backups before rewriting, fetched PRs),
//! - files under `.git/<name>/…` (the action journal).
//!
//! A namespace may also *read* the namespaces of other programs, so a repository used by
//! both keeps working (one program finds the other's backups). It never writes there.

use std::path::{Path, PathBuf};

/// A program's namespace inside a repository (seam N8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefNamespace {
    name: &'static str,
    reads: &'static [&'static str],
}

/// A namespace is one ref component of lower-case letters, digits and `-`, so it is valid in
/// a ref name and as a folder name on every system.
const fn valid(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || b[0] == b'-' {
        return false;
    }
    let mut i = 0;
    while i < b.len() {
        if !(b[i].is_ascii_lowercase() || b[i].is_ascii_digit() || b[i] == b'-') {
            return false;
        }
        i += 1;
    }
    true
}

impl RefNamespace {
    /// The namespace `name`, also reading `reads`. Panics (at compile time for a `const`) if
    /// a name is not lower-case letters, digits and `-`.
    pub const fn new(name: &'static str, reads: &'static [&'static str]) -> Self {
        assert!(
            valid(name),
            "a namespace is lower-case letters, digits and -"
        );
        let mut i = 0;
        while i < reads.len() {
            assert!(
                valid(reads[i]),
                "a namespace is lower-case letters, digits and -"
            );
            i += 1;
        }
        Self { name, reads }
    }

    /// The name, e.g. `gittailor`.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Namespaces of other programs this one reads too, never writes.
    pub const fn reads(&self) -> &'static [&'static str] {
        self.reads
    }

    /// Prefix of the backups made before an action moves refs: `refs/<name>/backup/`.
    pub fn backup_prefix(&self) -> String {
        format!("refs/{}/backup/", self.name)
    }

    /// The prefix of one backup run at `unix` seconds: `refs/<name>/backup/<unix>/` (a secret
    /// rescue saves all rewritten branches under one).
    pub fn backup_prefix_at(&self, unix: i64) -> String {
        format!("{}{unix}/", self.backup_prefix())
    }

    /// The backup prefixes this namespace reads: its own first, then those of `reads`.
    pub fn backup_prefixes(&self) -> impl Iterator<Item = String> + '_ {
        std::iter::once(self.name)
            .chain(self.reads.iter().copied())
            .map(|n| format!("refs/{n}/backup/"))
    }

    /// The backup ref of `what` (a short branch or tag name, or `HEAD`) at `unix` seconds:
    /// `refs/<name>/backup/<unix>/<what>`.
    pub fn backup_ref(&self, unix: i64, what: &str) -> String {
        format!("{}{what}", self.backup_prefix_at(unix))
    }

    /// The ref a PR's head is fetched into: `refs/<name>/review/<host>/<number>`.
    pub fn review_ref(&self, host: &str, number: u64) -> String {
        format!("refs/{}/review/{host}/{number}", self.name)
    }

    /// The ref the PR's target is fetched into next to the head:
    /// `refs/<name>/review/<host>/<number>-base` (not `<number>/base`, which Git cannot keep
    /// next to the ref `<number>`).
    pub fn review_base_ref(&self, host: &str, number: u64) -> String {
        format!("{}-base", self.review_ref(host, number))
    }

    /// The program's folder inside the Git directory: `<git_dir>/<name>`.
    pub fn data_dir(&self, git_dir: &Path) -> PathBuf {
        git_dir.join(self.name)
    }

    /// The action journal: `<git_dir>/<name>/actions.jsonl`.
    pub fn journal(&self, git_dir: &Path) -> PathBuf {
        self.data_dir(git_dir).join("actions.jsonl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: RefNamespace = RefNamespace::new("tool-a", &[]);
    const B: RefNamespace = RefNamespace::new("tool-b", &["tool-a"]);

    #[test]
    fn every_name_is_built_from_the_namespace() {
        assert_eq!(A.backup_prefix(), "refs/tool-a/backup/");
        assert_eq!(
            A.backup_ref(1_700_000_000, "main"),
            "refs/tool-a/backup/1700000000/main"
        );
        assert_eq!(A.backup_prefix_at(5), "refs/tool-a/backup/5/");
        assert_eq!(A.review_ref("github", 7), "refs/tool-a/review/github/7");
        assert_eq!(
            A.review_base_ref("gitlab", 7),
            "refs/tool-a/review/gitlab/7-base"
        );
        assert_eq!(
            A.journal(Path::new("/r/.git")),
            Path::new("/r/.git").join("tool-a").join("actions.jsonl")
        );
    }

    #[test]
    fn another_namespace_is_read_but_never_written() {
        let read: Vec<_> = B.backup_prefixes().collect();
        assert_eq!(read, ["refs/tool-b/backup/", "refs/tool-a/backup/"]);
        assert!(B.backup_ref(1, "main").starts_with("refs/tool-b/"));
        assert_eq!(A.backup_prefixes().count(), 1);
    }

    #[test]
    fn names_are_one_safe_component() {
        for ok in ["gittailor", "tool-2"] {
            assert!(valid(ok), "{ok}");
        }
        for bad in ["", "-x", "Tool", "a/b", "a.b", "a b", "ä"] {
            assert!(!valid(bad), "{bad:?}");
        }
        assert!(std::panic::catch_unwind(|| RefNamespace::new("a/b", &[])).is_err());
        assert!(std::panic::catch_unwind(|| RefNamespace::new("ok", &["../x"])).is_err());
    }
}

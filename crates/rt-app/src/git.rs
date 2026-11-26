// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Git for the open collection: the types both shells answer with, plus the one piece of Git
//! logic they share ([`text_diff`]).
//!
//! The Git work itself is I/O and lives in the shells: natively in the `rt-git` crate
//! (gitoxide), in the browser in `apps/web/src/git` (isomorphic-git over the File System
//! Access API, network through the companion). Both answer the `GitApi` commands of
//! `apps/ui/src/commands/backend.ts` with exactly these types, so the UI cannot tell them
//! apart.
//!
//! Model, kept small on purpose (wave 4):
//! - No staging area in the UI. `commit` takes the paths to include; everything else stays
//!   as it is in the working tree.
//! - Operations that change the working tree (`checkout`, `pull`, `resolve`) refuse to run
//!   while the workspace has unsaved changes (`AppError::Git`), and reopen the collection
//!   afterwards; the UI then re-reads the tree (`AppState.reload`).
//! - Paths are relative to the collection root, `/`-separated. The repository may start
//!   above the collection; files outside the collection are not listed.
//!
//! Owners in wave 4: strand A1 (native, `rt-git`), strand A2 (web and the Git UI).
//! Changes to these types are interface changes.
//!
//! Wave 12 (interface): revisions. `GitApi` reads any commit without a checkout:
//! `revParse`, `treeAt` ([`GitTree`]), `show`, `fileLog` ([`GitFileCommit`]), `mergeBase` and
//! `commits`. A *rev* is a full or abbreviated commit id, a local or remote branch (`main`,
//! `origin/main`), a ref under `refs/` or `HEAD`, optionally followed by `~<n>` or `^` (first
//! parent). Paths are collection-relative; a leading `/` makes a path relative to the
//! repository root (`show` only, e.g. an OpenAPI spec next to the collection folder).
//!
//! Wave 14 (interface): Git for any repository, not only for collections. [`graph`] is the
//! commit graph of the whole repository (all branches, remote branches, tags, `HEAD`; strand F),
//! [`actions`] the wizards' actions with a plan before and an undo entry after (strand G). Both
//! use repository-relative paths with a leading `/`, and nothing in them assumes the repository
//! is a collection: a collection file gets the semantic diff and merge, every other file the
//! line diff and merge. Several collections of the set can share one repository; Git works per
//! repository (the one of the active collection), and an action that changes the working tree
//! reloads all of them.
//!
//! Wave 15 (interface): a Git client for any repository. [`repos`] puts repositories without a
//! collection into the set (the *active repository* is what every `GitApi` command acts on;
//! strand A), [`files`] the whole working copy, a file tree and file contents for the viewers
//! (strand B), [`history`] blame, history search and the reflog (strand C), [`plumbing`]
//! in-memory replays and object writes for history editors, moved into place only by
//! [`actions::GitAction::Rewrite`] (strand D), [`index`] the index as building blocks for a
//! staging view and [`signing`] what the app knows about signatures (strand F). Everything new
//! uses repository-relative paths with a leading `/`; the default commit stays "without
//! staging".

pub mod actions;
pub mod files;
pub mod graph;
pub mod history;
pub mod index;
pub mod plumbing;
pub mod repos;
pub mod signing;

/// RequestTailor's namespace inside a repository (seam N8): `refs/requesttailor/…` and
/// `.git/requesttailor/…`. The Git core builds every ref and file of its own from it.
pub const REF_NAMESPACE: tk_git_types::RefNamespace =
    tk_git_types::RefNamespace::new("requesttailor", &[]);

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// State of the repository that contains the open collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatus {
    /// `false`: the collection is not in a Git repository (all other fields are empty).
    pub repo: bool,
    /// Current branch; `None` for a detached HEAD or an empty repository without commits.
    pub branch: Option<String>,
    /// Upstream of the current branch, e.g. `origin/main`.
    pub upstream: Option<String>,
    /// Commits on the branch that are not on the upstream, and the other way round.
    pub ahead: u32,
    pub behind: u32,
    /// Changed, new and conflicted files of the collection, sorted by path.
    pub files: Vec<GitFile>,
    /// A merge is in progress (after a pull with conflicts).
    pub merging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFile {
    pub path: String,
    pub state: GitFileState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitFileState {
    /// Not tracked yet.
    Untracked,
    /// Tracked and changed (content or mode).
    Modified,
    /// Deleted in the working tree.
    Deleted,
    /// Both sides changed it in a merge; see `GitApi.conflict`.
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBranch {
    /// Short name, e.g. `main`, or `origin/main` for a remote branch.
    pub name: String,
    pub current: bool,
    pub remote: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommit {
    /// Full hex object id.
    pub oid: String,
    /// First line of the message.
    pub summary: String,
    pub author: String,
    /// Author time, Unix seconds.
    pub time: i64,
}

/// The collection's files at a commit, read from Git's object store (no checkout): the same
/// files [`rt_format::FileSet::from_dir`] reads from disk (`*.yml` below the collection root,
/// without hidden folders, `node_modules` and nested collections). Collection files are UTF-8
/// text; a file that is not is left out.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitTree {
    /// The commit the rev resolved to (full hex id).
    pub oid: String,
    /// Path relative to the collection root → text.
    pub files: BTreeMap<String, String>,
}

impl GitTree {
    pub fn to_file_set(&self) -> rt_format::FileSet {
        rt_format::FileSet {
            files: self
                .files
                .iter()
                .map(|(p, t)| (p.clone(), t.as_bytes().to_vec()))
                .collect(),
        }
    }

    pub fn into_file_set(self) -> rt_format::FileSet {
        rt_format::FileSet {
            files: self
                .files
                .into_iter()
                .map(|(p, t)| (p, t.into_bytes()))
                .collect(),
        }
    }
}

/// Whether a path is one [`GitTree`] holds (relative to the collection root): a `.yml` file
/// outside hidden folders and `node_modules`. Nested collections are checked by the caller,
/// which sees the whole tree.
pub fn tree_path(path: &str) -> bool {
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() && (part.starts_with('.') || part == "node_modules") {
            return false;
        }
        if parts.peek().is_none() {
            return part.ends_with(".yml") && !part.is_empty();
        }
    }
    false
}

/// Drops the files of nested collections (a folder below the root with its own
/// `opencollection.yml`) from `files`, as [`rt_format::FileSet::from_dir`] skips them.
pub fn drop_nested(files: &mut BTreeMap<String, String>) {
    let nested: Vec<String> = files
        .keys()
        .filter_map(|p| p.strip_suffix(&format!("/{}", rt_format::COLLECTION_FILE)))
        .map(|dir| format!("{dir}/"))
        .collect();
    if !nested.is_empty() {
        files.retain(|p, _| !nested.iter().any(|dir| p.starts_with(dir)));
    }
}

/// How a commit of [`GitApi.fileLog`] touched the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitFileChange {
    Added,
    Modified,
    Deleted,
    /// Moved from [`GitFileCommit::from`] (with or without changes).
    Renamed,
}

/// A commit that changed a file, newest first in `fileLog`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileCommit {
    pub commit: GitCommit,
    /// The file's path at this commit (it differs from today's after a rename).
    pub path: String,
    pub change: GitFileChange,
    /// The path before a rename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// What `pull` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum GitPullResult {
    UpToDate,
    /// The branch moved forward to the upstream.
    FastForward {
        commits: u32,
    },
    /// A merge commit was created.
    Merged {
        oid: String,
    },
    /// The merge stopped; these files need a decision (`GitApi.resolve`).
    Conflicts {
        paths: Vec<String>,
    },
}

/// Both sides of a conflicted file, for the conflict view. `None` where a side deleted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitConflict {
    pub path: String,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

/// How to resolve a conflicted file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitResolution {
    Ours,
    Theirs,
}

/// Author identity for commits. Natively read from the Git configuration; in the browser
/// set once by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

/// Line diff of one file between `HEAD` and the working tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiff {
    pub path: String,
    /// Not text (or not UTF-8): no hunks.
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    /// 1-based first line and line count on each side, as in `@@ -a,b +c,d @@`.
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// The line without its line break.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

/// Lines of context around each change.
pub const DIFF_CONTEXT: usize = 3;

/// Line diff of `old` → `new` (either `None` for a missing side), the way both shells show
/// it. Hunks carry [`DIFF_CONTEXT`] lines of context.
pub fn text_diff(path: &str, old: Option<&[u8]>, new: Option<&[u8]>) -> GitDiff {
    fn text(b: Option<&[u8]>) -> Option<&str> {
        match b {
            None => Some(""),
            Some(b) if b.contains(&0) => None,
            Some(b) => std::str::from_utf8(b).ok(),
        }
    }
    let (Some(old), Some(new)) = (text(old), text(new)) else {
        return GitDiff {
            path: path.to_owned(),
            binary: true,
            hunks: Vec::new(),
        };
    };
    let diff = similar::TextDiff::from_lines(old, new);
    let hunks = diff
        .grouped_ops(DIFF_CONTEXT)
        .into_iter()
        .map(|group| {
            let (first, last) = (&group[0], &group[group.len() - 1]);
            let old_range = first.old_range().start..last.old_range().end;
            let new_range = first.new_range().start..last.new_range().end;
            let lines = group
                .iter()
                .flat_map(|op| diff.iter_changes(op))
                .map(|change| DiffLine {
                    kind: match change.tag() {
                        similar::ChangeTag::Equal => DiffLineKind::Context,
                        similar::ChangeTag::Insert => DiffLineKind::Added,
                        similar::ChangeTag::Delete => DiffLineKind::Removed,
                    },
                    text: change
                        .value()
                        .trim_end_matches('\n')
                        .trim_end_matches('\r')
                        .to_owned(),
                })
                .collect();
            let start = |r: &std::ops::Range<usize>| {
                // Unified-diff convention: an empty side starts at the line before.
                if r.is_empty() { r.start } else { r.start + 1 }
            };
            DiffHunk {
                old_start: start(&old_range) as u32,
                old_lines: old_range.len() as u32,
                new_start: start(&new_range) as u32,
                new_lines: new_range.len() as u32,
                lines,
            }
        })
        .collect();
    GitDiff {
        path: path.to_owned(),
        binary: false,
        hunks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_paths_are_the_files_from_dir_reads() {
        assert!(tree_path("opencollection.yml"));
        assert!(tree_path("users/get.yml"));
        assert!(!tree_path("openapi.yaml"));
        assert!(!tree_path(".github/workflows/ci.yml"));
        assert!(!tree_path("node_modules/x/a.yml"));
        assert!(tree_path(".hidden.yml"));
        let mut files: BTreeMap<String, String> = [
            "opencollection.yml",
            "a.yml",
            "sub/opencollection.yml",
            "sub/b.yml",
            "subway/c.yml",
        ]
        .iter()
        .map(|p| (p.to_string(), String::new()))
        .collect();
        drop_nested(&mut files);
        let left: Vec<&str> = files.keys().map(String::as_str).collect();
        assert_eq!(left, vec!["a.yml", "opencollection.yml", "subway/c.yml"]);
    }

    fn kinds(d: &GitDiff) -> Vec<(DiffLineKind, &str)> {
        d.hunks
            .iter()
            .flat_map(|h| h.lines.iter().map(|l| (l.kind, l.text.as_str())))
            .collect()
    }

    #[test]
    fn one_changed_line_with_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nd\nE\nf\ng\nh\n";
        let d = text_diff("x.yml", Some(old.as_bytes()), Some(new.as_bytes()));
        assert!(!d.binary);
        assert_eq!(d.hunks.len(), 1);
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (2, 7, 2, 7)
        );
        use DiffLineKind::*;
        assert_eq!(
            kinds(&d),
            vec![
                (Context, "b"),
                (Context, "c"),
                (Context, "d"),
                (Removed, "e"),
                (Added, "E"),
                (Context, "f"),
                (Context, "g"),
                (Context, "h"),
            ]
        );
    }

    #[test]
    fn new_and_deleted_files() {
        let d = text_diff("n.yml", None, Some(b"x\ny\n"));
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (0, 0, 1, 2)
        );
        let d = text_diff("n.yml", Some(b"x\n"), None);
        assert_eq!(kinds(&d), vec![(DiffLineKind::Removed, "x")]);
        assert!(
            text_diff("same", Some(b"a\n"), Some(b"a\n"))
                .hunks
                .is_empty()
        );
    }

    #[test]
    fn binary_files_have_no_hunks() {
        let d = text_diff("b.bin", Some(b"a\0b"), Some(b"c"));
        assert!(d.binary);
        assert!(d.hunks.is_empty());
    }
}

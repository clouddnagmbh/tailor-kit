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

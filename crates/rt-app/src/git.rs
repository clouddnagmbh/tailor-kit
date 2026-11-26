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

/// The wire types live in the Tailor Kit (`tk_git_types`); these are the old paths.
pub use tk_git_types::{
    DIFF_CONTEXT, DiffHunk, DiffLine, DiffLineKind, GitBranch, GitCommit, GitConflict, GitDiff,
    GitFile, GitFileChange, GitFileCommit, GitFileState, GitIdentity, GitPullResult, GitResolution,
    GitStatus, GitTree, NoExtra, text_diff,
};

/// RequestTailor's own field on a changed file ([`graph::GitChangedFile`]) and a file of the
/// working copy ([`files::GitWorkingFile`]): the collection of the set the file belongs to.
/// Serialized after the kit's fields, as `collection` always was.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InCollection {
    /// `CollectionEntry::id`; `None`: any other file, shown as a line diff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
}

/// A [`GitTree`] as the collection's files: the same files [`rt_format::FileSet::from_dir`]
/// reads from disk (`*.yml` below the collection root, without hidden folders, `node_modules`
/// and nested collections), when the repository was opened with RequestTailor's
/// `CollectionFiles` (the default of `rt_git`).
pub trait GitTreeFiles {
    fn to_file_set(&self) -> rt_format::FileSet;
    fn into_file_set(self) -> rt_format::FileSet;
}

impl GitTreeFiles for GitTree {
    fn to_file_set(&self) -> rt_format::FileSet {
        rt_format::FileSet {
            files: self
                .files
                .iter()
                .map(|(p, t)| (p.clone(), t.as_bytes().to_vec()))
                .collect(),
        }
    }

    fn into_file_set(self) -> rt_format::FileSet {
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
}

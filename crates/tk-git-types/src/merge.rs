// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Seam N1: merging files on their content model instead of line by line.
//!
//! The Git core merges every file line by line (Git's own merge). A [`FileMerger`] plugged
//! into it takes over the files it claims: it names the folders whose files merge together
//! (a *group*, e.g. one API collection), and merges a group's three sides at once. Its result
//! replaces Git's for every file it decides; a file it leaves (`whole_file`) keeps Git's
//! result with conflict markers. Without a merger every file is merged line by line.

use std::collections::{BTreeMap, BTreeSet};

/// The files of one group by path relative to the group's folder (`/`-separated).
pub type GroupFiles = BTreeMap<String, Vec<u8>>;

/// A group merged by a [`FileMerger`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupMerge {
    /// Every file the merger decides (on any side); the others keep Git's result.
    pub paths: BTreeSet<String>,
    /// The merged files; a decided path that is missing here is deleted. A file with
    /// conflicts holds ours in each conflicted part.
    pub files: GroupFiles,
    /// What the working tree shows for a file with conflicts (e.g. ours with a comment above
    /// each conflicted part); falls back to `files`.
    pub marked: GroupFiles,
    /// Files with conflicts the merger understands, with the conflicted parts in plain words
    /// (a field path, or `file`). They get the conflict stages in the index.
    pub conflicts: BTreeMap<String, Vec<String>>,
    /// Files the merger cannot merge and leaves to Git as a whole (markers, Git's stages).
    pub whole_file: BTreeSet<String>,
}

/// Merges groups of files on their content model (seam N1).
pub trait FileMerger: Send + Sync {
    /// The folders (repository paths, `""` for the root) whose files merge as one group,
    /// from every file path on any side.
    fn groups(&self, paths: &[&str]) -> BTreeSet<String>;

    /// Whether a path relative to a group's folder belongs to the group. Only these files
    /// reach [`FileMerger::merge`].
    fn claims(&self, rel: &str) -> bool;

    /// Merges one group; `None` leaves the whole group to Git (e.g. a side cannot be read).
    fn merge(
        &self,
        base: &GroupFiles,
        ours: &GroupFiles,
        theirs: &GroupFiles,
    ) -> Option<GroupMerge>;

    /// How many parts of each file changed from `base` to `theirs`, for a merge plan
    /// ("3 fields merge cleanly"). Files without an entry count as one part.
    fn changed_parts(&self, base: &GroupFiles, theirs: &GroupFiles) -> BTreeMap<String, u32> {
        let _ = (base, theirs);
        BTreeMap::new()
    }
}

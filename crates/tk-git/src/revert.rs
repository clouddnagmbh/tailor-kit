// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Revert (wave 14, strand G): a new commit on the current branch that takes back the change
//! of one commit. The same three-way merge as cherry-pick (`cherry_pick.rs`), turned around:
//! the commit is the base and its parent is "theirs". History stays as it is.

use gix::ObjectId;

use crate::merge::{MergeKind, ThreeWay};

/// The merge that takes `commit` (with first parent `parent`) back on top of `head`.
pub(crate) fn three_way<'a>(
    head: ObjectId,
    commit: ObjectId,
    parent: ObjectId,
    label: &'a str,
) -> ThreeWay<'a> {
    ThreeWay {
        kind: MergeKind::Revert,
        base: Some(commit),
        ours: head,
        theirs: parent,
        label,
    }
}

/// Git's message for a revert.
pub(crate) fn message(subject: &str, commit: ObjectId) -> String {
    format!("Revert \"{subject}\"\n\nThis reverts commit {commit}.\n")
}

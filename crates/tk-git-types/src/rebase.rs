// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Rebase as a sequence of picks in memory (GitTailor wave 3, strand A): the types of
//! [`GitAction::Rebase`](crate::actions::GitAction::Rebase) and of a rebase that stopped.
//!
//! The engine replays the steps one by one in an in-memory object store (as the dry run does);
//! only when every step went through does the branch move, once, with a backup ref and a journal
//! entry (undo puts it back). When a step stops (a conflict, an `edit` step, a `reword` without a
//! message), the state is written in Git's own format, `.git/rebase-merge/` (`head-name`, `onto`,
//! `orig-head`, `git-rebase-todo`, `done`, `msgnum`, `end`, `interactive`, `stopped-sha`, `amend`,
//! `message`), with the conflict stages in the index and conflict markers in the files, so
//! `git status` reports "interactive rebase in progress" and `git rebase --continue`/`--abort`
//! take over just as GitTailor's own continue and abort do; a rebase started with `git rebase`
//! is continued the same way. Commits replayed unchanged on the same parent keep their ids (as
//! `git rebase` does without `--force-rebase`).

use serde::{Deserialize, Serialize};

/// What a step does with its commit (the words of `git rebase -i`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitRebaseStepKind {
    Pick,
    /// Pick with a new message (`message`; without one the rebase stops to ask).
    Reword,
    /// Pick, then stop so the commit can be changed (continue amends it).
    Edit,
    /// Fold into the commit before, messages combined (`message` replaces the combination).
    Squash,
    /// Fold into the commit before, keeping that commit's message.
    Fixup,
    /// Leave the commit out.
    Drop,
}

/// One line of the todo, applied in order (oldest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRebaseStep {
    pub kind: GitRebaseStepKind,
    /// Full commit id.
    pub commit: String,
    /// `reword`, `squash`: the new message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// How a step went in the plan's in-memory run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRebasePlanStep {
    pub step: GitRebaseStep,
    pub subject: String,
    /// The commit it produces (`None`: dropped, folded into the next, or not reached).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    /// Files that conflict at this step (repository-relative, leading `/`); the run stops here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
    /// The step changes nothing on the new base (already upstream); left out like `git rebase`.
    #[serde(default)]
    pub empty: bool,
}

/// [`GitPlan::rebase`](crate::actions::GitPlan::rebase): the resolved todo and how far it goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRebasePreview {
    /// The branch that moves (short name).
    pub branch: String,
    /// The commit the steps are replayed on.
    pub onto: String,
    /// The last commit not replayed (merge base, or the given upstream).
    pub base: String,
    pub steps: Vec<GitRebasePlanStep>,
    /// Index of the first step that stops (conflict, `edit`, `reword` without a message).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stops_at: Option<usize>,
    /// The branch's new tip when no step stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip: Option<String>,
}

/// Why a rebase in progress stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitRebaseStop {
    Conflict,
    Edit,
    /// A `reword` (or `squash`) that waits for its message.
    Message,
    /// Stopped by someone else (`git rebase` on the command line, `break`, `exec`).
    Other,
}

/// A rebase in progress, read from `.git/rebase-merge/` (also one `git rebase` started).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRebaseState {
    /// `head-name` (short); `None`: a detached rebase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub onto: String,
    /// The branch's tip before the rebase (`orig-head`; abort goes back there).
    pub orig_head: String,
    pub done: Vec<GitRebaseStep>,
    pub todo: Vec<GitRebaseStep>,
    /// The commit of the step that stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    pub stop: GitRebaseStop,
    /// Conflicted files (repository-relative, leading `/`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
}

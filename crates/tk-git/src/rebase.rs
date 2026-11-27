// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Rebase as picks in memory (GitTailor wave 3, strand A; interface stub). The plan, the run,
//! continue, skip and the state of a stopped rebase in `.git/rebase-merge/`: see
//! [`tk_git_types::rebase`]. Until the strand lands, every rebase action plans blocked.

use tk_git_types::rebase::GitRebaseState;

use crate::repo::Repo;
use crate::util::{GitError, Result};

impl Repo {
    /// The rebase in progress (GitTailor's or one `git rebase` started), read from
    /// `.git/rebase-merge/`; `None` when there is none.
    pub fn rebase_state(&self) -> Result<Option<GitRebaseState>> {
        Err(GitError::Unsupported(
            "reading a rebase in progress (GitTailor wave 3, strand A)".into(),
        ))
    }
}

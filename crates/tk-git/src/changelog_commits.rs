// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The commits of a changelog's range with their full messages (wave 16, strand D).

use crate::repo::Repo;
use crate::util::{Context, Result};

/// A commit with its whole message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMessage {
    /// Full hex id.
    pub oid: String,
    /// The message as stored.
    pub message: String,
    /// The author's name.
    pub author: String,
}

impl Repo {
    /// Commits reachable from `to` along first parents but not from `from`, newest first, with
    /// their full messages (what a changelog is written from).
    pub fn changelog_messages(&self, from: &str, to: &str) -> Result<Vec<CommitMessage>> {
        let to = self.resolve_rev(to)?;
        let from = self.resolve_rev(from)?;
        let walk = self
            .repo
            .rev_walk([to])
            .first_parent_only()
            .with_hidden([from])
            .all()
            .ctx("cannot walk the history")?;
        let mut out = Vec::new();
        for info in walk {
            let info = info.ctx("cannot walk the history")?;
            let commit = self.repo.find_commit(info.id).ctx("cannot read a commit")?;
            let decoded = commit.decode().ctx("cannot read a commit")?;
            let author = decoded.author().ctx("cannot read a commit")?;
            out.push(CommitMessage {
                oid: info.id.to_string(),
                message: decoded.message.to_string(),
                author: author.name.to_string(),
            });
        }
        Ok(out)
    }
}

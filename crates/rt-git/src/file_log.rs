// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitApi.fileLog`: the commits that changed one file of the collection, newest first.
//!
//! Interface state (wave 12): by path only, without following renames. Owner: strand D, who
//! adds rename following (the web shell's isomorphic-git `log({ follow })` already follows).

use gix::bstr::BString;
use rt_app::git::{GitFileChange, GitFileCommit};

use crate::commit::describe;
use crate::repo::Repo;
use crate::util::{Context, Result};

impl Repo {
    /// Commits of `HEAD`'s history (by commit time) in which `path` (collection-relative)
    /// differs from the commit's first parent, at most `limit`.
    pub fn file_log(&self, path: &str, limit: u32) -> Result<Vec<GitFileCommit>> {
        let Some(head) = self.head_commit()? else {
            return Ok(Vec::new());
        };
        let repo_path: BString = self.scope.abs(path)?;
        let walk = self
            .repo
            .rev_walk([head])
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                Default::default(),
            ))
            .all()
            .ctx("cannot walk the history")?;
        let mut out = Vec::new();
        for info in walk {
            if out.len() >= limit as usize {
                break;
            }
            let info = info.ctx("cannot walk the history")?;
            let here = self.entry_at(info.id, &repo_path)?.map(|(_, id)| id);
            let parent = match info.parent_ids.first() {
                Some(p) => self.entry_at(*p, &repo_path)?.map(|(_, id)| id),
                None => None,
            };
            let change = match (parent, here) {
                (None, Some(_)) => GitFileChange::Added,
                (Some(_), None) => GitFileChange::Deleted,
                (Some(a), Some(b)) if a != b => GitFileChange::Modified,
                _ => continue,
            };
            out.push(GitFileCommit {
                commit: describe(&self.repo, info.id)?,
                path: path.to_owned(),
                change,
                from: None,
            });
        }
        Ok(out)
    }
}

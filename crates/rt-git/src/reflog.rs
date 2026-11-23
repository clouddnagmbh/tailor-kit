// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitApi.reflog` (wave 15, strand C): where `HEAD` or a branch pointed before, newest first.
//! The reflog records everything Git did to the ref, also on the command line, so it is how
//! lost work comes back (`GitAction::RestoreBranch` of strand D takes an id from here).

use gix::ObjectId;
use gix::bstr::ByteSlice;
use rt_app::git::history::GitReflogEntry;

use crate::commit::describe;
use crate::repo::Repo;
use crate::util::{Context, Result, fail};

impl Repo {
    /// The full name of `name`: `HEAD`, a full ref, or a short branch name.
    fn reflog_name(&self, name: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(fail("no ref given"));
        }
        if name == "HEAD" || name.starts_with("refs/") {
            return Ok(name.to_owned());
        }
        Ok(format!("refs/heads/{name}"))
    }

    /// The reflog of `name` (`HEAD`, `refs/heads/main` or `main`), newest first, at most
    /// `limit` entries; empty when the ref has no reflog.
    pub fn reflog(&self, name: &str, limit: u32) -> Result<Vec<GitReflogEntry>> {
        let full = self.reflog_name(name)?;
        let Some(reference) = self
            .repo
            .try_find_reference(full.as_str())
            .ctx("cannot read the reflog")?
        else {
            return Ok(Vec::new());
        };
        let mut platform = reference.log_iter();
        let Some(lines) = platform.rev().ctx("cannot read the reflog")? else {
            return Ok(Vec::new());
        };
        let null = ObjectId::null(self.repo.object_hash());
        let mut out = Vec::new();
        for (index, line) in lines.take(limit as usize).enumerate() {
            let line = line.ctx("cannot read the reflog")?;
            let new = line.new_oid;
            let old = line.previous_oid;
            out.push(GitReflogEntry {
                name: full.clone(),
                index: index as u32,
                old: (old != null).then(|| old.to_string()),
                new: new.to_string(),
                message: line.message.to_str_lossy().trim().to_owned(),
                time: line.signature.time.seconds,
                commit: describe(&self.repo, new).ok(),
            });
        }
        Ok(out)
    }
}

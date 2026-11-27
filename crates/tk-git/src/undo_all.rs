// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Undo for everything (GitTailor wave 2, strand B): changes that are no `GitAction` (staging,
//! taking lines back, resolving a conflict) get a journal entry too ([`Repo::record`]), an undo
//! can itself be undone (the redo, [`Repo::undo_recorded`]), and any entry says whether it can
//! still be undone ([`Repo::action`]).
//!
//! A recorded change is journaled like an action: the refs it moved, `HEAD`, the configuration,
//! the stash list, the remote-tracking refs, the working files (when it may change them) and the
//! index, staged changes and conflict stages included. [`Repo::undo`] takes it back and is
//! refused once any of it changed since, so newer work is never lost.

use gix::bstr::ByteSlice;
use tk_git_types::actions::{GitActionResult, GitUndoEntry};

use crate::actions::{Recorded, now};
use crate::credentials::CredentialSource;
use crate::repo::Repo;
use crate::util::{Context, Result, fail, flatten};
use crate::worktree::blocked;

impl Repo {
    /// Every local branch and tag, the stash, `HEAD` and the current branch: what a recorded
    /// change may move.
    fn local_ref_names(&self) -> Result<Vec<String>> {
        let mut names = vec!["HEAD".to_owned()];
        if let Some(h) = self.head_ref()? {
            names.push(h.as_bstr().to_str_lossy().into_owned());
        }
        let refs = self.repo.references().ctx("cannot list the refs")?;
        for r in refs.all().ctx("cannot list the refs")? {
            let r = r.map_err(|e| fail(format!("cannot list the refs: {e}")))?;
            let name = r.name().as_bstr().to_str_lossy().into_owned();
            if (name.starts_with("refs/heads/")
                || name.starts_with("refs/tags/")
                || name == "refs/stash")
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        if !names.iter().any(|n| n == "refs/stash") {
            names.push("refs/stash".into());
        }
        Ok(names)
    }

    /// Runs `change` and journals what it changed as the entry `action` (`summary` in plain
    /// words), so [`Repo::undo`] takes it back like an action: for staging, discarding lines,
    /// resolving a conflict, or anything else that is no `GitAction`. `files`: it may change
    /// working files (they are kept as a commit before, and undo is refused once one of them
    /// was edited since). The index is always part of the entry. Nothing changed: no entry.
    pub fn record<T>(
        &self,
        action: &str,
        summary: &str,
        files: bool,
        change: impl FnOnce(&Repo) -> Result<T>,
    ) -> Result<(T, Option<GitUndoEntry>)> {
        self.record_with(action, summary, files, None, change)
    }

    fn record_with<T>(
        &self,
        action: &str,
        summary: &str,
        files: bool,
        pushed: Option<crate::remote::Pushed>,
        change: impl FnOnce(&Repo) -> Result<T>,
    ) -> Result<(T, Option<GitUndoEntry>)> {
        let names = self.local_ref_names()?;
        let message = format!("{}: working files before {action}", self.product().name);
        let whole = self.whole()?;
        let before = self.capture(names, files.then_some(message.as_str()), &whole)?;
        let time = now();
        let out = change(self)?;
        let entry = self.journal_after(
            before,
            Recorded {
                action,
                summary,
                time,
                backups: Vec::new(),
                pushed,
                fetch: false,
                index_moved: true,
            },
        )?;
        Ok((out, entry))
    }

    /// [`Repo::undo_with`], journaled itself: [`GitActionResult::undo`] is the entry of the
    /// undo (action `undo`), whose own undo is the redo: it brings back the state after the
    /// original action (a push or publish is sent again, with a lease, where the product opts
    /// in with [`Repo::with_push_undo`]).
    pub fn undo_recorded(
        &self,
        id: &str,
        unsaved: bool,
        creds: &dyn CredentialSource,
    ) -> Result<GitActionResult> {
        let line = self
            .journal()?
            .into_iter()
            .find(|l| l.entry.id == id)
            .ok_or_else(|| fail("This action can no longer be undone."))?;
        // The undo of a push sets the remote back; its redo sets it forward again.
        let pushed = line.restore.pushed.as_ref().map(|p| crate::remote::Pushed {
            remote: p.remote.clone(),
            remote_ref: p.remote_ref.clone(),
            before: p.after.clone(),
            after: p.before.clone(),
        });
        let summary = format!("Undone: {}", line.entry.summary);
        let (mut result, entry) = self.record_with("undo", &summary, true, pushed, |r| {
            r.undo_with(id, unsaved, creds)
        })?;
        result.undo = entry;
        Ok(result)
    }

    /// The journal entry `id` with `stale` set when its undo would be refused now (refs moved,
    /// `HEAD` elsewhere, the configuration, the stash list or the staged changes changed, or a
    /// working file it wrote was edited since); `None` once it left the journal.
    pub fn action(&self, id: &str) -> Result<Option<GitUndoEntry>> {
        let Some(line) = self.journal()?.into_iter().find(|l| l.entry.id == id) else {
            return Ok(None);
        };
        let mut entry = line.entry.clone();
        entry.stale = self.staleness(&line)?;
        if entry.stale.is_none()
            && let Some((before, after)) = &line.restore.worktree
        {
            let whole = self.whole()?;
            let before = gix::ObjectId::from_hex(before.as_bytes()).ctx("invalid journal")?;
            let after = gix::ObjectId::from_hex(after.as_bytes()).ctx("invalid journal")?;
            let from = flatten(&whole.repo, after)?;
            let to = flatten(&whole.repo, whole.commit_tree(before)?)?;
            let index = whole.load_index()?;
            let stuck = blocked(&whole, &from, &to, &index)?;
            if !stuck.is_empty() {
                let mut names: Vec<String> = stuck
                    .iter()
                    .take(3)
                    .map(|p| p.to_str_lossy().into_owned())
                    .collect();
                if stuck.len() > 3 {
                    names.push(format!("{} more", stuck.len() - 3));
                }
                entry.stale = Some(format!(
                    "{} changed since, so undoing would lose newer work.",
                    names.join(", ")
                ));
            }
        }
        Ok(Some(entry))
    }
}

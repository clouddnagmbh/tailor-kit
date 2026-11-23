// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitAction::Abort` (wave 15, strand D): stop the merge, cherry-pick or revert in progress,
//! or a rebase started on the command line, and go back to where it started, like
//! `git merge --abort` and `git rebase --abort`.
//!
//! Files the operation changed or the user resolved go back to the last commit (the branch's
//! own, or for a rebase the one it started from). Files Git does not know (not in the index)
//! stay. The working files before are kept under a backup ref, so Undo brings them back as
//! plain changes; it cannot resume the operation itself.

use std::collections::BTreeSet;

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use rt_app::git::GitFileState;
use rt_app::git::actions::{GitBlockReason, GitPlan, GitRefMove};

use crate::actions::{Ctx, Done, PlanExt, block, plural_s};
use crate::repo::{Repo, Underway};
use crate::util::{Context, Result, fail, flatten};

/// A rebase that stopped (started by `git rebase`).
struct Rebase {
    /// Full name of the branch it works on; `None` when detached.
    branch: Option<String>,
    /// Where the branch was before the rebase.
    orig_head: ObjectId,
    dir: std::path::PathBuf,
}

impl Repo {
    fn rebase_underway(&self) -> Result<Option<Rebase>> {
        for name in ["rebase-merge", "rebase-apply"] {
            let dir = self.git_dir().join(name);
            if !dir.is_dir() {
                continue;
            }
            let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
            let (Some(orig), head_name) = (read("orig-head"), read("head-name")) else {
                continue;
            };
            let Ok(orig_head) = ObjectId::from_hex(orig.trim().as_bytes()) else {
                continue;
            };
            let branch = head_name
                .map(|h| h.trim().to_owned())
                .filter(|h| h.starts_with("refs/"));
            return Ok(Some(Rebase {
                branch,
                orig_head,
                dir,
            }));
        }
        Ok(None)
    }

    pub(crate) fn plan_abort(&self, cx: &Ctx, mut p: GitPlan) -> Result<GitPlan> {
        let rebase = self.rebase_underway()?;
        let what = match (&cx.underway, &rebase) {
            (Some(Underway::Merge), _) => "merge",
            (Some(Underway::CherryPick), _) => "cherry-pick",
            (Some(Underway::Revert), _) => "revert",
            (None, Some(_)) => "rebase",
            (None, None) => {
                p.undoable = false;
                return Ok(p.blocked_by(block(
                    GitBlockReason::NothingToDo,
                    "Nothing is in progress that could be stopped.",
                )));
            }
        };
        let files = cx.tracked_changes()?;
        match &rebase {
            Some(r) if cx.underway.is_none() => {
                let to = r
                    .branch
                    .as_deref()
                    .and_then(|b| b.strip_prefix("refs/heads/"))
                    .map(str::to_owned)
                    .unwrap_or_else(|| self.named(r.orig_head));
                p.steps.push(format!(
                    "Stops the rebase and puts {to} back where it was before it: {}.",
                    self.named(r.orig_head)
                ));
            }
            _ => p.steps.push(format!(
                "Stops the {what} and puts your files back the way they were before it started. Your branch does not move."
            )),
        }
        if !files.is_empty() {
            p.steps.push(format!(
                "The changes in {} are taken out of your working files.",
                plural_s(files.len(), "file")
            ));
            p.warnings.push(
                "Undo brings your files back as plain changes; it cannot resume the operation."
                    .into(),
            );
            p.destructive = true;
        }
        p.files = files.iter().map(|f| format!("/{}", f.path)).collect();
        if let Some(r) = &rebase
            && cx.underway.is_none()
            && let Some(full) = &r.branch
        {
            let before = self.ref_value(full)?;
            if before != Some(r.orig_head) {
                p.refs.push(GitRefMove {
                    name: full.clone(),
                    before: before.map(|id| id.to_string()),
                    after: Some(r.orig_head.to_string()),
                });
            }
        }
        p.backup = true;
        p.stop(cx.unsaved());
        Ok(p)
    }

    pub(crate) fn run_abort(&self, cx: &Ctx) -> Result<Done> {
        let rebase = self.rebase_underway()?;
        let whole = &cx.whole;
        // Back to this commit's files: `HEAD`'s, or where a rebase began.
        let target = match (&cx.underway, &rebase) {
            (None, Some(r)) => r.orig_head,
            _ => cx
                .head
                .ok_or_else(|| fail("There is nothing to go back to."))?,
        };
        let target_tree = self.commit_tree(target)?;
        let now = crate::undo::snapshot_tree(whole)?;
        let index = whole.load_index()?;
        let known: BTreeSet<BString> = index
            .entries()
            .iter()
            .map(|e| e.path(&index).to_owned())
            .collect();
        let now_flat = flatten(&self.repo, now)?;
        let target_flat = flatten(&self.repo, target_tree)?;
        let mut editor = self
            .repo
            .edit_tree(target_tree)
            .ctx("cannot build the tree")?;
        for f in cx.changes()? {
            let path = BString::from(f.path.as_str());
            if f.state == GitFileState::Untracked
                && !known.contains(&path)
                && !target_flat.contains_key(&path)
                && let Some(blob) = now_flat.get(&path)
            {
                editor
                    .upsert(
                        path.as_bstr(),
                        crate::commit::entry_kind(blob.mode),
                        blob.id,
                    )
                    .ctx("cannot build the tree")?;
            }
        }
        let want = editor.write().ctx("cannot write the tree")?.detach();
        let message = "requesttailor: abort";
        if let Some(r) = &rebase
            && cx.underway.is_none()
        {
            match &r.branch {
                Some(full) => {
                    let name = gix::refs::FullName::try_from(full.as_str())
                        .map_err(|_| fail("invalid branch name in the rebase"))?;
                    let current = self.ref_value(full)?;
                    self.put_ref(full, Some(r.orig_head), current, message)?;
                    self.set_head(&name, message)?;
                }
                None => self.detach_head(r.orig_head, message)?,
            }
        }
        self.clear_merge_state()?;
        // The files first against the index as it is, then the index for the new `HEAD`.
        let repo = self.reopen()?;
        let whole = repo.whole()?;
        whole.move_worktree(now, want, "abort")?;
        if let Some(r) = rebase
            && cx.underway.is_none()
        {
            std::fs::remove_dir_all(&r.dir).ctx("cannot clear the rebase state")?;
        }
        Ok(Done::new(
            if cx.underway.is_some() {
                "Stopped it and went back to where it started."
            } else {
                "Stopped the rebase and went back to where it started."
            },
            true,
        ))
    }
}

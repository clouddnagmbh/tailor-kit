// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitAction::Rewrite` (wave 15, strand D): point a branch at a commit a history editor
//! built from the old ones (`GitApi.pick`, `writeTree`, `writeCommit`). The plan says which
//! commits the branch loses and which it gets; it is refused when a commit the branch would
//! lose is on a remote-tracking branch ([`GitBlockReason::Published`]), or when `tip` does
//! not exist. When the branch is checked out, the working files follow (refused with local
//! changes in the way); Undo puts the branch and the files back. Nothing else moves.

use gix::bstr::{BString, ByteSlice};
use tk_git_types::GitFileState;
use tk_git_types::actions::{GitBlockReason, GitPlan, GitRefMove};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural};
use crate::branch_actions::{branch_full, diff_paths};
use crate::repo::Repo;
use crate::util::{Result, fail, flatten};

impl Repo {
    pub(crate) fn plan_rewrite(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        branch: &str,
        tip: &str,
        summary: &str,
    ) -> Result<GitPlan> {
        let full = branch_full(branch);
        let Some(old) = self.ref_commit(&full)? else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no branch \"{branch}\"."),
            )));
        };
        let Some(new) = self.try_resolve(tip) else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("The new tip \"{tip}\" does not exist."),
            )));
        };
        if new == old {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                format!("{branch} would not change."),
            )));
        }
        let leaving = self.count_hidden(old, &[new])?;
        let coming = self.count_hidden(new, &[old])?;
        let published = self.published_in(old, &[new])?;
        if published > 0 {
            p.stop(Some(block(
                GitBlockReason::Published,
                format!(
                    "{} you would replace {} on the server already. Rewriting history others may have is not done from here.",
                    plural(published, "commit", "commits"),
                    if published == 1 { "is" } else { "are" }
                ),
            )));
        }
        if !summary.trim().is_empty() {
            p.steps
                .push(format!("{}.", summary.trim().trim_end_matches('.')));
        }
        p.steps.push(format!(
            "Replaces {} on {branch} with {}; {branch} then points at {}.",
            plural(leaving, "commit", "commits"),
            coming,
            self.named(new)
        ));
        p.steps
            .push("The backup keeps the old commits for Undo.".into());
        p.refs.push(GitRefMove {
            name: full,
            before: Some(old.to_string()),
            after: Some(new.to_string()),
        });
        p.backup = true;
        if cx.cur.as_deref() == Some(branch) {
            p.files = diff_paths(&self.flat_of(Some(old))?, &self.flat_of(Some(new))?);
            p.stop(cx.unsaved());
            p.stop(cx.in_progress()?);
            p.stop(cx.local_changes("rewrite this branch")?);
        }
        Ok(p)
    }

    pub(crate) fn run_rewrite(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        branch: &str,
        tip: &str,
        summary: &str,
    ) -> Result<Done> {
        let full = branch_full(branch);
        let old = self
            .ref_commit(&full)?
            .ok_or_else(|| fail(format!("There is no branch \"{branch}\".")))?;
        let new = self.resolve_rev(tip)?;
        run.backup(self, branch, Some(old))?;
        let message = self.reflog_message(&format!("rewrite {branch}"));
        let text = if summary.trim().is_empty() {
            format!("Rewrote {branch}.")
        } else {
            format!("{}.", summary.trim().trim_end_matches('.'))
        };
        if cx.cur.as_deref() != Some(branch) {
            self.put_ref(&full, Some(new), Some(old), &message)?;
            return Ok(Done::new(text, false));
        }
        let whole = &cx.whole;
        let now = crate::undo::snapshot_tree(whole)?;
        // The new files, and the untracked files the new commit does not have.
        let new_tree = self.commit_tree(new)?;
        let new_flat = flatten(&self.repo, new_tree)?;
        let now_flat = flatten(&self.repo, now)?;
        let mut editor = self
            .repo
            .edit_tree(new_tree)
            .map_err(|e| fail(format!("cannot build the tree: {e}")))?;
        for f in cx.changes()? {
            let path = BString::from(f.path.as_str());
            if f.state == GitFileState::Untracked
                && !new_flat.contains_key(&path)
                && let Some(blob) = now_flat.get(&path)
            {
                editor
                    .upsert(
                        path.as_bstr(),
                        crate::commit::entry_kind(blob.mode),
                        blob.id,
                    )
                    .map_err(|e| fail(format!("cannot build the tree: {e}")))?;
            }
        }
        let want = editor
            .write()
            .map_err(|e| fail(format!("cannot write the tree: {e}")))?
            .detach();
        self.put_ref(&full, Some(new), Some(old), &message)?;
        if let Err(e) = whole.reopen()?.move_worktree(now, want, "rewrite") {
            let _ = self.put_ref(
                &full,
                Some(old),
                Some(new),
                &self.reflog_message("rewrite undone"),
            );
            return Err(e);
        }
        Ok(Done::new(text, true))
    }
}

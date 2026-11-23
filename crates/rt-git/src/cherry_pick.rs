// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Cherry-pick (wave 14, strand G), and the plan and run that revert shares: the change of
//! one commit merged onto `HEAD` with the wave-12 machinery (`merge.rs`: semantic for
//! collection files, line by line for every other file) and committed with one parent. A
//! conflict stops like a pull's (`CHERRY_PICK_HEAD`/`REVERT_HEAD`, conflict stages, markers),
//! so the conflict views take over and the next commit concludes it.
//!
//! The picked commit keeps its author and message; a revert is by the user.

use gix::ObjectId;
use rt_app::git::GitPullResult;
use rt_app::git::actions::{GitBlockReason, GitMergePreview, GitPlan, GitRefMove};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural, plural_s};
use crate::branch_actions::branch_full;
use crate::merge::{MergeKind, ThreeWay};
use crate::repo::Repo;
use crate::util::{Context, Result, fail};

/// The merge that applies `commit` (first parent `parent`) on top of `head`.
fn pick_three_way<'a>(
    head: ObjectId,
    commit: ObjectId,
    parent: Option<ObjectId>,
    label: &'a str,
) -> ThreeWay<'a> {
    ThreeWay {
        kind: MergeKind::CherryPick,
        base: parent,
        ours: head,
        theirs: commit,
        label,
    }
}

impl Repo {
    fn parents_of(&self, id: ObjectId) -> Result<Vec<ObjectId>> {
        let commit = self.repo.find_commit(id).ctx("cannot read a commit")?;
        Ok(commit.parent_ids().map(|p| p.detach()).collect())
    }

    /// Plan of cherry-pick (`pick`) or revert.
    pub(crate) fn plan_pick(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        commit: &str,
        pick: bool,
    ) -> Result<GitPlan> {
        p.stop(cx.need_branch());
        let Some(oid) = self.try_resolve(commit) else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no commit \"{commit}\"."),
            )));
        };
        let (Some(head), Some(cur)) = (cx.head, cx.cur.as_deref()) else {
            return Ok(p);
        };
        let parents = self.parents_of(oid)?;
        if parents.len() > 1 {
            return Ok(p.blocked_by(block(
                GitBlockReason::Unsupported,
                "This is a merge commit. Undoing or copying merges works on the command line for now.",
            )));
        }
        if parents.is_empty() && !pick {
            return Ok(p.blocked_by(block(
                GitBlockReason::Unsupported,
                "This is the very first commit; undoing it would remove everything.",
            )));
        }
        if pick && self.is_ancestor(oid, head) {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                format!("{cur} contains this commit already."),
            )));
        }
        let label = oid.to_hex_with_len(7).to_string();
        let tw = if pick {
            pick_three_way(head, oid, parents.first().copied(), &label)
        } else {
            crate::revert::three_way(head, oid, parents[0], &label)
        };
        let mem = self.repo.clone().with_object_memory();
        let computed = self.compute_merge(&mem, &tw)?;
        let files = self.merge_preview(computed)?;
        let subject = self.subject(oid);
        p.steps.push(if pick {
            format!("Copies the changes of \"{subject}\" onto {cur} as a new commit.")
        } else {
            format!(
                "Makes a new commit on {cur} that takes back \"{subject}\". History stays as it is."
            )
        });
        let conflicts = files.iter().filter(|f| !f.conflicts.is_empty()).count();
        if conflicts > 0 {
            p.steps.push(format!(
                "{} your decision afterwards.",
                plural(conflicts, "file needs", "files need")
            ));
        }
        if files.is_empty() {
            p.stop(Some(block(
                GitBlockReason::NothingToDo,
                if pick {
                    "These changes are on your branch already."
                } else {
                    "Your branch no longer has these changes."
                },
            )));
        }
        p.files = files.iter().map(|f| f.path.clone()).collect();
        p.merge = Some(GitMergePreview {
            fast_forward: false,
            commits: vec![crate::commit::describe(&self.repo, oid)?],
            files,
        });
        p.refs.push(GitRefMove {
            name: branch_full(cur),
            before: Some(head.to_string()),
            after: Some(String::new()),
        });
        p.backup = true;
        p.stop(cx.unsaved());
        p.stop(cx.in_progress()?);
        p.stop(cx.local_changes(if pick {
            "copy a commit"
        } else {
            "undo a commit"
        })?);
        Ok(p)
    }

    /// Run of cherry-pick (`pick`) or revert.
    pub(crate) fn run_pick(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        commit: &str,
        pick: bool,
    ) -> Result<Done> {
        let (head, cur) = match (cx.head, cx.cur.clone()) {
            (Some(h), Some(c)) => (h, c),
            _ => return Err(fail("You are not on a branch.")),
        };
        let oid = self.resolve_rev(commit)?;
        let parents = self.parents_of(oid)?;
        let label = oid.to_hex_with_len(7).to_string();
        let subject = self.subject(oid);
        let picked = self.repo.find_commit(oid).ctx("cannot read a commit")?;
        let (tw, message, author) = if pick {
            let decoded = picked.decode().ctx("cannot read a commit")?;
            let author: gix::actor::Signature = decoded
                .author()
                .ctx("cannot read a commit")?
                .to_owned()
                .ctx("cannot read a commit")?;
            (
                pick_three_way(head, oid, parents.first().copied(), &label),
                decoded.message.to_string(),
                Some(author),
            )
        } else {
            let parent = *parents
                .first()
                .ok_or_else(|| fail("This is the very first commit."))?;
            (
                crate::revert::three_way(head, oid, parent, &label),
                crate::revert::message(&subject, oid),
                None,
            )
        };
        let identity = self.identity_or(run.opts.identity)?;
        run.backup(self, &cur, Some(head))?;
        let computed = self.compute_merge(&self.repo, &tw)?;
        let what = if pick { "cherry-pick" } else { "revert" };
        let outcome = self.apply_merge(computed, &tw, &identity, author, &message, what)?;
        let text = match &outcome {
            GitPullResult::Conflicts { paths } => format!(
                "{} \"{subject}\" needs your decision in {}.",
                if pick { "Copying" } else { "Undoing" },
                plural_s(paths.len(), "file")
            ),
            _ if pick => format!("Copied \"{subject}\" onto {cur}."),
            _ => format!("Undid \"{subject}\" with a new commit."),
        };
        Ok(Done {
            message: text,
            outcome: Some(outcome),
            reload: true,
        })
    }
}

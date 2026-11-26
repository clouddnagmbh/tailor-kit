// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The Git wizards' actions (wave 14, strand G; types and rules in `rt_app::git::actions`):
//! [`Repo::plan`] says what an action would do without changing anything, [`Repo::apply`]
//! plans again, refuses what is blocked and runs it, [`Repo::undo`] takes it back.
//!
//! Actions work on the whole repository, not only the collection: paths in plans are
//! repository-relative with a leading `/`. (Conflict paths in [`GitPullResult`] stay as the
//! pull has them: collection-relative inside the collection, `/…` outside it.)
//!
//! Safety net, before anything moves:
//! - a backup ref `refs/requesttailor/backup/<unix>/<name>` for every branch, tag, stash or
//!   `HEAD` the action moves away from (and `…/worktree`, a commit of the working files, when
//!   it throws files away), as [`GitPlan::backup`] says;
//! - an entry in the action journal `.git/requesttailor/actions.jsonl` (newest last, at most
//!   [`UNDO_KEEP`]), with what undo needs: the refs before and after, where `HEAD` pointed,
//!   the working files before and after (as commits/trees), the repository configuration and
//!   the stash list when the action changed them.
//!
//! Network actions (fetch, push, publish, push-tag) are not undoable and write no entry.
//! Unsaved changes in the app are the caller's to know: it passes [`ActionOptions::unsaved`].

use std::cell::OnceCell;

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};
use rt_app::git::actions::{
    GitAction, GitActionResult, GitBlock, GitBlockReason, GitPlan, GitRefMove, GitUndoEntry,
    UNDO_KEEP, backup_ref,
};
use rt_app::git::{GitCommit, GitFile, GitFileState, GitIdentity, GitPullResult};
use serde::{Deserialize, Serialize};

use crate::credentials::CredentialSource;
use crate::repo::{Repo, Underway};
use crate::util::{Context, GitError, Result, fail};

/// What an action may use besides the repository.
pub struct ActionOptions<'a> {
    /// The app has unsaved edits: actions that change files are refused.
    pub unsaved: bool,
    /// For fetch, pull, push, publish and push-tag.
    pub credentials: &'a dyn CredentialSource,
    /// The app's own identity, for commits when the Git configuration has none.
    pub identity: Option<&'a GitIdentity>,
}

pub(crate) fn block(reason: GitBlockReason, message: impl Into<String>) -> GitBlock {
    GitBlock {
        reason,
        message: message.into(),
    }
}

/// `1 commit`, `2 commits`.
pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub(crate) fn plural_s(n: usize, one: &str) -> String {
    plural(n, one, &format!("{one}s"))
}

pub(crate) fn short(id: &ObjectId) -> String {
    id.to_hex_with_len(7).to_string()
}

/// Repository path → the plans' form (leading `/`).
pub(crate) fn slash(path: &[u8]) -> String {
    format!("/{}", path.to_str_lossy())
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Blocks in order: the first one set stays.
pub(crate) trait PlanExt {
    fn stop(&mut self, b: Option<GitBlock>);
    fn blocked_by(self, b: GitBlock) -> GitPlan;
}

impl PlanExt for GitPlan {
    fn stop(&mut self, b: Option<GitBlock>) {
        if self.blocked.is_none() {
            self.blocked = b;
        }
    }
    fn blocked_by(mut self, b: GitBlock) -> GitPlan {
        self.stop(Some(b));
        self
    }
}

pub(crate) fn new_plan(action: &GitAction) -> GitPlan {
    GitPlan {
        action: action.clone(),
        steps: Vec::new(),
        refs: Vec::new(),
        files: Vec::new(),
        network: false,
        backup: false,
        undoable: true,
        merge: None,
        destructive: false,
        warnings: Vec::new(),
        blocked: None,
    }
}

/// The repository as an action finds it.
pub(crate) struct Ctx {
    pub unsaved: bool,
    /// The commit `HEAD` points to.
    pub head: Option<ObjectId>,
    /// The branch `HEAD` points to (also without commits); `None` when detached.
    pub head_ref: Option<FullName>,
    /// The current branch when it has commits (short name).
    pub cur: Option<String>,
    pub underway: Option<Underway>,
    /// The repository without the collection scope.
    pub whole: Repo,
    changes: OnceCell<Vec<GitFile>>,
}

impl Ctx {
    /// Changed, deleted, untracked and conflicted files of the whole repository
    /// (repository-relative, no leading `/`).
    pub fn changes(&self) -> Result<&[GitFile]> {
        if let Some(c) = self.changes.get() {
            return Ok(c);
        }
        let files = self.whole.status()?.files;
        Ok(self.changes.get_or_init(|| files))
    }

    /// Files that differ from the last commit (untracked files do not count: Git keeps them).
    pub fn tracked_changes(&self) -> Result<Vec<&GitFile>> {
        Ok(self
            .changes()?
            .iter()
            .filter(|f| f.state != GitFileState::Untracked)
            .collect())
    }

    pub fn unsaved(&self) -> Option<GitBlock> {
        self.unsaved.then(|| {
            block(
                GitBlockReason::UnsavedChanges,
                "Save your changes in the app first.",
            )
        })
    }

    pub fn in_progress(&self) -> Result<Option<GitBlock>> {
        let conflicts = self
            .changes()?
            .iter()
            .any(|f| f.state == GitFileState::Conflicted);
        if self.underway.is_none() && !conflicts {
            return Ok(None);
        }
        Ok(Some(block(
            GitBlockReason::InProgress,
            if conflicts {
                "A merge is waiting for your decision: resolve the conflicts and commit first."
            } else {
                "Commit to finish the merge in progress first."
            },
        )))
    }

    pub fn need_branch(&self) -> Option<GitBlock> {
        match (&self.cur, &self.head_ref) {
            (Some(_), _) => None,
            (None, Some(_)) => Some(block(
                GitBlockReason::Detached,
                "Make the first commit first.",
            )),
            (None, None) => Some(block(
                GitBlockReason::Detached,
                "You are not on a branch. Create a branch here or switch to one first.",
            )),
        }
    }

    /// Blocks while files differ from the last commit.
    pub fn local_changes(&self, what: &str) -> Result<Option<GitBlock>> {
        let n = self.tracked_changes()?.len();
        Ok((n > 0).then(|| {
            block(
                GitBlockReason::LocalChanges,
                format!(
                    "You have {}. Commit or park them before you {what}.",
                    plural_s(n, "uncommitted change")
                ),
            )
        }))
    }
}

/// What a run did.
pub(crate) struct Done {
    pub message: String,
    pub outcome: Option<GitPullResult>,
    pub reload: bool,
}

impl Done {
    pub fn new(message: impl Into<String>, reload: bool) -> Self {
        Self {
            message: message.into(),
            outcome: None,
            reload,
        }
    }
}

/// The running action: options, time and the backups written so far.
pub(crate) struct Run<'a> {
    pub opts: &'a ActionOptions<'a>,
    pub time: i64,
    pub backups: Vec<String>,
}

impl Run<'_> {
    /// Writes `refs/requesttailor/backup/<time>/<name>` → `id` (a numbered name when taken).
    pub fn backup(&mut self, repo: &Repo, name: &str, id: Option<ObjectId>) -> Result<()> {
        let Some(id) = id else { return Ok(()) };
        for n in 1..100 {
            let full = if n == 1 {
                backup_ref(self.time, name)
            } else {
                backup_ref(self.time, &format!("{name}-{n}"))
            };
            if repo
                .repo
                .try_find_reference(full.as_str())
                .ctx("cannot read a backup")?
                .is_some()
            {
                continue;
            }
            repo.repo
                .reference(
                    full.as_str(),
                    id,
                    PreviousValue::MustNotExist,
                    "requesttailor: backup before a Git action",
                )
                .ctx("cannot write the backup ref")?;
            self.backups.push(full);
            return Ok(());
        }
        Err(fail("cannot write the backup ref"))
    }
}

/// What undo needs besides [`GitUndoEntry::refs`] (private to the journal).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Restore {
    /// Full name of the branch `HEAD` pointed to before and after (`None`: detached).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<(Option<String>, Option<String>)>,
    /// The working files: a commit of them before, the tree of them after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<(String, String)>,
    /// `.git/config` before and after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<(String, String)>,
    /// The stash's reflog before and after (`""`: none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stash_log: Option<(String, String)>,
    /// Remote-tracking refs a remote action moved (not checked for staleness).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_refs: Vec<GitRefMove>,
    /// The action left a merge (cherry-pick, revert) in progress.
    #[serde(default)]
    pub underway: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JournalLine {
    #[serde(flatten)]
    pub entry: GitUndoEntry,
    #[serde(default)]
    pub restore: Restore,
}

impl Repo {
    pub(crate) fn journal_path(&self) -> std::path::PathBuf {
        self.git_dir().join("requesttailor").join("actions.jsonl")
    }

    /// The journal, oldest first; unreadable lines are skipped.
    pub(crate) fn journal(&self) -> Result<Vec<JournalLine>> {
        let text = match std::fs::read_to_string(self.journal_path()) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(fail(format!("cannot read the action journal: {e}"))),
        };
        Ok(text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect())
    }

    /// Writes the journal, keeping the newest [`UNDO_KEEP`] lines.
    pub(crate) fn write_journal(&self, lines: &[JournalLine]) -> Result<()> {
        let path = self.journal_path();
        let io = |e: std::io::Error| fail(format!("cannot write the action journal: {e}"));
        std::fs::create_dir_all(path.parent().expect("has a parent")).map_err(io)?;
        let skip = lines.len().saturating_sub(UNDO_KEEP);
        let mut out = String::new();
        for line in &lines[skip..] {
            out.push_str(&serde_json::to_string(line).ctx("cannot write the action journal")?);
            out.push('\n');
        }
        let tmp = path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, out).map_err(io)?;
        std::fs::rename(&tmp, &path).map_err(io)
    }

    /// The raw value of a ref by full name: what a branch, tag or the stash points to (a tag
    /// object for annotated tags); for `HEAD`, the commit.
    pub(crate) fn ref_value(&self, name: &str) -> Result<Option<ObjectId>> {
        if name == "HEAD" {
            return self.head_commit();
        }
        let Some(mut r) = self
            .repo
            .try_find_reference(name)
            .ctx("cannot read a ref")?
        else {
            return Ok(None);
        };
        match r.target() {
            gix::refs::TargetRef::Object(id) => Ok(Some(id.to_owned())),
            gix::refs::TargetRef::Symbolic(_) => {
                Ok(Some(r.peel_to_id().ctx("cannot read a ref")?.detach()))
            }
        }
    }

    /// Sets (`Some`) or deletes (`None`) the ref `name`, if it still is `expected`.
    pub(crate) fn put_ref(
        &self,
        name: &str,
        value: Option<ObjectId>,
        expected: Option<ObjectId>,
        message: &str,
    ) -> Result<()> {
        let full = FullName::try_from(name).map_err(|_| fail(format!("invalid ref {name}")))?;
        let prev = match expected {
            Some(id) => PreviousValue::MustExistAndMatch(Target::Object(id)),
            None => PreviousValue::MustNotExist,
        };
        let change = match value {
            Some(id) => Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: message.into(),
                },
                expected: prev,
                new: Target::Object(id),
            },
            None => {
                if expected.is_none() {
                    return Ok(());
                }
                Change::Delete {
                    expected: prev,
                    log: RefLog::AndReference,
                }
            }
        };
        self.repo
            .edit_reference(RefEdit {
                change,
                name: full,
                deref: false,
            })
            .ctx(&format!("cannot update {name}"))?;
        Ok(())
    }

    /// Points `HEAD` at a commit directly (detached).
    pub(crate) fn detach_head(&self, id: ObjectId, message: &str) -> Result<()> {
        self.repo
            .edit_reference(RefEdit {
                change: Change::Update {
                    log: LogChange {
                        mode: RefLog::AndReference,
                        force_create_reflog: false,
                        message: message.into(),
                    },
                    expected: PreviousValue::Any,
                    new: Target::Object(id),
                },
                name: "HEAD".try_into().expect("valid"),
                deref: false,
            })
            .ctx("cannot move HEAD")?;
        Ok(())
    }

    /// `rev` as a commit, `None` if it names none.
    pub(crate) fn try_resolve(&self, rev: &str) -> Option<ObjectId> {
        self.resolve_rev(rev).ok()
    }

    /// First line of a commit's message.
    pub(crate) fn subject(&self, id: ObjectId) -> String {
        crate::commit::describe(&self.repo, id)
            .map(|c| c.summary)
            .unwrap_or_default()
    }

    /// `a1b2c3d ("Subject")`.
    pub(crate) fn named(&self, id: ObjectId) -> String {
        format!("{} (\"{}\")", short(&id), self.subject(id))
    }

    /// Commits reachable from `tip` but not from `hidden`, newest first, at most `limit`.
    pub(crate) fn commits_between(
        &self,
        tip: ObjectId,
        hidden: &[ObjectId],
        limit: usize,
    ) -> Result<Vec<GitCommit>> {
        let walk = self
            .repo
            .rev_walk([tip])
            .with_hidden(hidden.iter().copied())
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                Default::default(),
            ))
            .all()
            .ctx("cannot walk the history")?;
        let mut out = Vec::new();
        for info in walk.take(limit) {
            let info = info.ctx("cannot walk the history")?;
            out.push(crate::commit::describe(&self.repo, info.id)?);
        }
        Ok(out)
    }

    /// How many commits are reachable from `tip` but not from `hidden`.
    pub(crate) fn count_hidden(&self, tip: ObjectId, hidden: &[ObjectId]) -> Result<usize> {
        let walk = self
            .repo
            .rev_walk([tip])
            .with_hidden(hidden.iter().copied())
            .all()
            .ctx("cannot walk the history")?;
        let mut n = 0;
        for info in walk {
            info.ctx("cannot walk the history")?;
            n += 1;
        }
        Ok(n)
    }

    pub(crate) fn is_ancestor(&self, ancestor: ObjectId, tip: ObjectId) -> bool {
        crate::local::is_ancestor(&self.repo, ancestor, tip)
    }

    pub(crate) fn ctx(&self, unsaved: bool) -> Result<Ctx> {
        let head = self.head_commit()?;
        let head_ref = self.head_ref()?;
        let cur = match head {
            Some(_) => self.head_branch()?,
            None => None,
        };
        Ok(Ctx {
            unsaved,
            head,
            head_ref,
            cur,
            underway: self.underway()?,
            whole: self.whole()?,
            changes: OnceCell::new(),
        })
    }

    /// What `action` would do now, and whether it can (reads only).
    pub fn plan(&self, action: &GitAction, unsaved: bool) -> Result<GitPlan> {
        let cx = self.ctx(unsaved)?;
        self.plan_with(&cx, action)
    }

    fn plan_with(&self, cx: &Ctx, action: &GitAction) -> Result<GitPlan> {
        let p = new_plan(action);
        match action {
            GitAction::CreateBranch {
                name,
                start,
                switch,
            } => self.plan_create_branch(cx, p, name, start, *switch),
            GitAction::Switch { target } => self.plan_switch(cx, p, target),
            GitAction::RenameBranch { from, to } => self.plan_rename_branch(cx, p, from, to),
            GitAction::DeleteBranch { name, force } => self.plan_delete_branch(cx, p, name, *force),
            GitAction::Merge { from } => self.plan_merge(cx, p, from),
            GitAction::Pull => self.plan_pull(cx, p),
            GitAction::Push => self.plan_push(cx, p),
            GitAction::Publish { branch, remote } => self.plan_publish(cx, p, branch, remote),
            GitAction::Fetch { remote } => self.plan_fetch(p, remote.as_deref()),
            GitAction::AddRemote { name, url } => self.plan_add_remote(p, name, url),
            GitAction::RemoveRemote { name } => self.plan_remove_remote(p, name),
            GitAction::RenameRemote { from, to } => self.plan_rename_remote(p, from, to),
            GitAction::SetRemoteUrl { name, url } => self.plan_set_remote_url(p, name, url),
            GitAction::Stash { paths, .. } => self.plan_stash(cx, p, paths),
            GitAction::Unstash { index, keep } => self.plan_unstash(cx, p, *index, *keep),
            GitAction::DropStash { index } => self.plan_drop_stash(p, *index),
            GitAction::Revert { commit } => self.plan_pick(cx, p, commit, false),
            GitAction::CherryPick { commit } => self.plan_pick(cx, p, commit, true),
            GitAction::Reset { target, mode } => self.plan_reset(cx, p, target, *mode),
            GitAction::Discard { paths } => self.plan_discard(cx, p, paths),
            GitAction::CreateTag {
                name,
                target,
                message,
            } => self.plan_create_tag(p, name, target, message.as_deref()),
            GitAction::DeleteTag { name } => self.plan_delete_tag(p, name),
            GitAction::PushTag { name, remote } => self.plan_push_tag(p, name, remote),
            GitAction::Amend {
                message,
                paths,
                staged,
            } => self.plan_amend(cx, p, message.as_deref(), paths, *staged),
            GitAction::Abort => self.plan_abort(cx, p),
            GitAction::RestoreBranch { name, target } => {
                self.plan_restore_branch(cx, p, name, target)
            }
            GitAction::RestoreFiles { rev, paths } => self.plan_restore_files(cx, p, rev, paths),
            GitAction::Rewrite {
                branch,
                tip,
                summary,
            } => self.plan_rewrite(cx, p, branch, tip, summary),
        }
    }

    fn run_action(
        &self,
        cx: &Ctx,
        action: &GitAction,
        plan: &GitPlan,
        run: &mut Run<'_>,
    ) -> Result<Done> {
        match action {
            GitAction::CreateBranch {
                name,
                start,
                switch,
            } => self.run_create_branch(cx, run, name, start, *switch),
            GitAction::Switch { target } => self.run_switch(cx, run, target),
            GitAction::RenameBranch { from, to } => self.run_rename_branch(cx, from, to),
            GitAction::DeleteBranch { name, .. } => self.run_delete_branch(run, name),
            GitAction::Merge { from } => self.run_merge(cx, run, plan, from),
            GitAction::Pull => self.run_pull(cx, run),
            GitAction::Push => self.run_push(cx, run),
            GitAction::Publish { branch, remote } => self.run_publish(run, branch, remote),
            GitAction::Fetch { remote } => self.run_fetch(run, remote.as_deref()),
            GitAction::AddRemote { name, url } => self.run_add_remote(name, url),
            GitAction::RemoveRemote { name } => self.run_remove_remote(name),
            GitAction::RenameRemote { from, to } => self.run_rename_remote(from, to),
            GitAction::SetRemoteUrl { name, url } => self.run_set_remote_url(name, url),
            GitAction::Stash { message, paths } => self.run_stash(cx, message.as_deref(), paths),
            GitAction::Unstash { index, keep } => self.run_unstash(cx, run, *index, *keep),
            GitAction::DropStash { index } => self.run_drop_stash(run, *index),
            GitAction::Revert { commit } => self.run_pick(cx, run, commit, false),
            GitAction::CherryPick { commit } => self.run_pick(cx, run, commit, true),
            GitAction::Reset { target, mode } => self.run_reset(cx, run, target, *mode),
            GitAction::Discard { .. } => self.run_discard(cx, run, plan),
            GitAction::CreateTag {
                name,
                target,
                message,
            } => self.run_create_tag(run, name, target, message.as_deref()),
            GitAction::DeleteTag { name } => self.run_delete_tag(run, name),
            GitAction::PushTag { name, remote } => self.run_push_tag(run, name, remote),
            GitAction::Amend {
                message,
                paths,
                staged,
            } => self.run_amend(cx, run, message.as_deref(), paths, *staged),
            GitAction::Abort => self.run_abort(cx),
            GitAction::RestoreBranch { name, target } => self.run_restore_branch(run, name, target),
            GitAction::RestoreFiles { rev, paths } => self.run_restore_files(cx, rev, paths),
            GitAction::Rewrite {
                branch,
                tip,
                summary,
            } => self.run_rewrite(cx, run, branch, tip, summary),
        }
    }

    /// Refs the journal records for `plan`: what it names (not remote-tracking refs), `HEAD`,
    /// the current branch and the stash.
    fn recorded_refs(&self, plan: &GitPlan) -> Result<Vec<String>> {
        let mut names: Vec<String> = Vec::new();
        let mut add = |n: String| {
            if !names.contains(&n) {
                names.push(n);
            }
        };
        for r in &plan.refs {
            if !r.name.starts_with("refs/remotes/") {
                add(r.name.clone());
            }
        }
        add("HEAD".into());
        if let Some(h) = self.head_ref()? {
            add(h.as_bstr().to_str_lossy().into_owned());
        }
        add("refs/stash".into());
        Ok(names)
    }

    /// Plans `action` again and runs it when nothing blocks it (else `GitError::Failed` with the
    /// plan's message). Undoable actions write backups and a journal entry first.
    pub fn apply(&self, action: &GitAction, opts: &ActionOptions<'_>) -> Result<GitActionResult> {
        let cx = self.ctx(opts.unsaved)?;
        let plan = self.plan_with(&cx, action)?;
        if let Some(b) = &plan.blocked {
            return Err(GitError::Failed(b.message.clone()));
        }
        let time = now();
        let mut run = Run {
            opts,
            time,
            backups: Vec::new(),
        };
        if !plan.undoable {
            let done = self.run_action(&cx, action, &plan, &mut run)?;
            return Ok(GitActionResult {
                message: done.message,
                undo: None,
                outcome: done.outcome,
                reload: done.reload,
            });
        }
        let names = self.recorded_refs(&plan)?;
        let before: Vec<Option<ObjectId>> = names
            .iter()
            .map(|n| self.ref_value(n))
            .collect::<Result<_>>()?;
        let head_before = self.head_ref()?.map(|n| n.as_bstr().to_string());
        let config_before = self.config_text()?;
        let stash_before = self.stash_log_text()?;
        let remote_before = self.remote_refs()?;
        let worktree_before = if touches_worktree(action, &plan) {
            let id = crate::undo::snapshot_commit(
                &cx.whole,
                &format!("RequestTailor: working files before {}", action_tag(action)),
            )?;
            if worktree_backup(action) {
                run.backup(self, "worktree", Some(id))?;
            }
            Some(id)
        } else {
            None
        };

        let done = self.run_action(&cx, action, &plan, &mut run)?;

        // A fresh view: the run may have changed the configuration and HEAD.
        let after_repo = self.reopen()?;
        let mut refs = Vec::new();
        for (name, b) in names.iter().zip(&before) {
            let a = after_repo.ref_value(name)?;
            if a != *b {
                refs.push(GitRefMove {
                    name: name.clone(),
                    before: b.map(|id| id.to_string()),
                    after: a.map(|id| id.to_string()),
                });
            }
        }
        let head_after = after_repo.head_ref()?.map(|n| n.as_bstr().to_string());
        let config_after = after_repo.config_text()?;
        let stash_after = after_repo.stash_log_text()?;
        let mut restore = Restore {
            head: (head_before != head_after).then_some((head_before, head_after)),
            config: (config_before != config_after).then_some((config_before, config_after)),
            stash_log: (stash_before != stash_after).then_some((stash_before, stash_after)),
            underway: after_repo.underway()?.is_some(),
            ..Restore::default()
        };
        if let Some(before) = worktree_before {
            let whole = after_repo.whole()?;
            let after = crate::undo::snapshot_tree(&whole)?;
            restore.worktree = Some((before.to_string(), after.to_string()));
        }
        let remote_after = after_repo.remote_refs()?;
        if remote_before != remote_after {
            let mut all: Vec<&String> = remote_before.keys().chain(remote_after.keys()).collect();
            all.sort();
            all.dedup();
            for name in all {
                let (b, a) = (remote_before.get(name), remote_after.get(name));
                if b != a {
                    restore.remote_refs.push(GitRefMove {
                        name: name.clone(),
                        before: b.map(|id| id.to_string()),
                        after: a.map(|id| id.to_string()),
                    });
                }
            }
        }
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let entry = GitUndoEntry {
            id: format!("{time}-{nanos:09}-{seq}"),
            action: action_tag(action).to_owned(),
            summary: done.message.clone(),
            time,
            refs,
            backups: run.backups.clone(),
            stash: None,
            stale: None,
        };
        let mut journal = self.journal()?;
        journal.push(JournalLine {
            entry: entry.clone(),
            restore,
        });
        self.write_journal(&journal)?;
        Ok(GitActionResult {
            message: done.message,
            undo: Some(entry),
            outcome: done.outcome,
            reload: done.reload,
        })
    }

    /// The newest journal entry, with `stale` set when undo would be refused.
    pub fn last_action(&self) -> Result<Option<GitUndoEntry>> {
        let Some(line) = self.journal()?.pop() else {
            return Ok(None);
        };
        let mut entry = line.entry.clone();
        entry.stale = self.staleness(&line)?;
        Ok(Some(entry))
    }
}

/// The `GitAction` tag (`create-branch`, …).
pub(crate) fn action_tag(action: &GitAction) -> &'static str {
    match action {
        GitAction::CreateBranch { .. } => "create-branch",
        GitAction::Switch { .. } => "switch",
        GitAction::RenameBranch { .. } => "rename-branch",
        GitAction::DeleteBranch { .. } => "delete-branch",
        GitAction::Merge { .. } => "merge",
        GitAction::Pull => "pull",
        GitAction::Push => "push",
        GitAction::Publish { .. } => "publish",
        GitAction::Fetch { .. } => "fetch",
        GitAction::AddRemote { .. } => "add-remote",
        GitAction::RemoveRemote { .. } => "remove-remote",
        GitAction::RenameRemote { .. } => "rename-remote",
        GitAction::SetRemoteUrl { .. } => "set-remote-url",
        GitAction::Stash { .. } => "stash",
        GitAction::Unstash { .. } => "unstash",
        GitAction::DropStash { .. } => "drop-stash",
        GitAction::Revert { .. } => "revert",
        GitAction::CherryPick { .. } => "cherry-pick",
        GitAction::Reset { .. } => "reset",
        GitAction::Discard { .. } => "discard",
        GitAction::CreateTag { .. } => "create-tag",
        GitAction::DeleteTag { .. } => "delete-tag",
        GitAction::PushTag { .. } => "push-tag",
        GitAction::Amend { .. } => "amend",
        GitAction::Abort => "abort",
        GitAction::RestoreBranch { .. } => "restore-branch",
        GitAction::RestoreFiles { .. } => "restore-files",
        GitAction::Rewrite { .. } => "rewrite",
    }
}

/// Whether the action may change working files (undo then restores them).
fn touches_worktree(action: &GitAction, plan: &GitPlan) -> bool {
    match action {
        GitAction::CreateBranch { switch, .. } => *switch && !plan.files.is_empty(),
        GitAction::Switch { .. }
        | GitAction::Merge { .. }
        | GitAction::Pull
        | GitAction::Stash { .. }
        | GitAction::Unstash { .. }
        | GitAction::Revert { .. }
        | GitAction::CherryPick { .. }
        | GitAction::Discard { .. } => true,
        GitAction::Reset { mode, .. } => *mode == rt_app::git::actions::GitResetMode::Discard,
        GitAction::Abort | GitAction::RestoreFiles { .. } => true,
        GitAction::Rewrite { .. } => !plan.files.is_empty(),
        _ => false,
    }
}

/// Whether the working files before the action get a backup ref of their own (the actions
/// that throw uncommitted work away; the others keep it in the files or the stash).
fn worktree_backup(action: &GitAction) -> bool {
    matches!(
        action,
        GitAction::Unstash { .. }
            | GitAction::Discard { .. }
            | GitAction::Reset { .. }
            | GitAction::Abort
            | GitAction::RestoreFiles { .. }
    )
}

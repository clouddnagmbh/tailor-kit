// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Wizard actions on branches and the current branch (wave 14, strand G): create, switch,
//! rename, delete, merge, pull, push, reset and discard. Plans read only; runs are called by
//! [`Repo::apply`] after the backups are written.

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::refs::FullName;
use tk_git_types::actions::{
    GitBlock, GitBlockReason, GitMergeFile, GitMergePreview, GitPlan, GitRefMove, GitResetMode,
    check_ref_name,
};
use tk_git_types::{GitFileState, GitPullResult};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural, plural_s, short, slash};
use crate::merge::{MergeKind, ThreeWay};
use crate::repo::Repo;
use crate::util::{Context, Flat, Result, fail, flatten};
use crate::worktree::{FileState, Probe, blocked};

/// Where a switch goes.
enum Dest {
    /// A local branch (full name).
    Local(String, ObjectId),
    /// A new local branch `name` tracking `remote/name`.
    Track {
        remote: String,
        name: String,
        id: ObjectId,
    },
    /// The local branch that already follows the remote one.
    Follows(String, ObjectId),
    /// A tag or commit: `HEAD` detached.
    Detached(ObjectId),
}

impl Dest {
    fn id(&self) -> ObjectId {
        match self {
            Dest::Local(_, id) | Dest::Follows(_, id) | Dest::Detached(id) => *id,
            Dest::Track { id, .. } => *id,
        }
    }
}

pub(crate) fn branch_full(name: &str) -> String {
    format!("refs/heads/{name}")
}

/// Repository paths whose entry differs between two flattened trees.
pub(crate) fn diff_paths(a: &Flat, b: &Flat) -> Vec<String> {
    let mut out: Vec<String> = a
        .keys()
        .chain(b.keys())
        .filter(|p| a.get(*p) != b.get(*p))
        .map(|p| slash(p))
        .collect();
    out.sort();
    out.dedup();
    out
}

impl Repo {
    pub(crate) fn flat_of(&self, commit: Option<ObjectId>) -> Result<Flat> {
        match commit {
            Some(c) => flatten(&self.repo, self.commit_tree(c)?),
            None => Ok(Flat::new()),
        }
    }

    /// "Your changes to … would be overwritten" when moving the working tree from `from` to
    /// `to` would touch files with local changes.
    pub(crate) fn would_overwrite(
        &self,
        cx: &Ctx,
        from: &Flat,
        to: &Flat,
    ) -> Result<Option<GitBlock>> {
        let index = cx.whole.load_index()?;
        let paths = blocked(&cx.whole, from, to, &index)?;
        if paths.is_empty() {
            return Ok(None);
        }
        let mut names: Vec<String> = paths.iter().take(5).map(|p| slash(p)).collect();
        if paths.len() > 5 {
            names.push(format!("and {} more", paths.len() - 5));
        }
        Ok(Some(block(
            GitBlockReason::LocalChanges,
            format!(
                "Your changes to {} would be overwritten. Commit or park them first.",
                names.join(", ")
            ),
        )))
    }

    /// Moves the working tree from `HEAD`'s tree to `to`'s (keeping local changes of other
    /// files) and writes the index for `to`.
    fn checkout_tree(&self, cx: &Ctx, to: ObjectId, what: &str) -> Result<()> {
        let from = cx.whole.head_tree()?;
        let mut index = crate::worktree::switch(&cx.whole, from, self.commit_tree(to)?, what)?;
        cx.whole.write_index(&mut index)
    }

    // ---- create-branch ------------------------------------------------------------------

    pub(crate) fn plan_create_branch(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        name: &str,
        start: &str,
        switch: bool,
    ) -> Result<GitPlan> {
        let start_id = self.try_resolve(start);
        p.refs.push(GitRefMove {
            name: branch_full(name),
            before: None,
            after: start_id.map(|id| id.to_string()),
        });
        p.steps.push(format!(
            "Creates the branch {name} at {}.",
            match start_id {
                Some(id) => self.named(id),
                None => start.to_owned(),
            }
        ));
        p.stop(check_ref_name(name).err());
        if check_ref_name(name).is_ok() && self.ref_value(&branch_full(name))?.is_some() {
            p.stop(Some(block(
                GitBlockReason::Exists,
                format!("A branch named {name} exists already. Choose another name."),
            )));
        }
        if start_id.is_none() {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no commit or branch \"{start}\"."),
            )));
        }
        if switch {
            p.steps
                .push(format!("Switches to {name}; new commits go there."));
            p.refs.push(GitRefMove {
                name: "HEAD".into(),
                before: cx.head.map(|id| id.to_string()),
                after: start_id.map(|id| id.to_string()),
            });
            if let Some(start_id) = start_id
                && Some(start_id) != cx.head
            {
                p.backup = true;
                p.stop(cx.unsaved());
                p.stop(cx.in_progress()?);
                let (from, to) = (self.flat_of(cx.head)?, self.flat_of(Some(start_id))?);
                p.stop(self.would_overwrite(cx, &from, &to)?);
                p.files = diff_paths(&from, &to);
            }
        }
        Ok(p)
    }

    pub(crate) fn run_create_branch(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        name: &str,
        start: &str,
        switch: bool,
    ) -> Result<Done> {
        let start_id = self.resolve_rev(start)?;
        let full = branch_full(name);
        self.put_ref(
            &full,
            Some(start_id),
            None,
            &format!("branch: Created from {start}"),
        )?;
        if !switch {
            return Ok(Done::new(format!("Created {name}."), false));
        }
        let moved = Some(start_id) != cx.head;
        if moved {
            run.backup(self, "HEAD", cx.head)?;
            self.checkout_tree(cx, start_id, "switch branches")?;
        }
        let full = FullName::try_from(full.as_str()).map_err(|_| fail("invalid branch name"))?;
        self.set_head(&full, &format!("checkout: moving to {name}"))?;
        Ok(Done::new(
            format!("Created {name} and switched to it."),
            moved,
        ))
    }

    // ---- switch ------------------------------------------------------------------------

    fn switch_dest(&self, target: &str) -> Result<Option<Dest>> {
        let local = branch_full(target);
        if check_ref_name(target).is_ok()
            && let Some(id) = self.ref_commit(&local)?
        {
            return Ok(Some(Dest::Local(local, id)));
        }
        if let Some((remote, name)) = target.split_once('/')
            && check_ref_name(name).is_ok()
            && let Some(id) = self.ref_commit(&format!("refs/remotes/{target}"))?
        {
            let full = branch_full(name);
            return Ok(Some(match self.ref_commit(&full)? {
                Some(existing) => Dest::Follows(full, existing),
                None => Dest::Track {
                    remote: remote.to_owned(),
                    name: name.to_owned(),
                    id,
                },
            }));
        }
        Ok(self.try_resolve(target).map(Dest::Detached))
    }

    pub(crate) fn plan_switch(&self, cx: &Ctx, mut p: GitPlan, target: &str) -> Result<GitPlan> {
        let Some(dest) = self.switch_dest(target)? else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no branch, tag or commit \"{target}\"."),
            )));
        };
        let current = cx.head_ref.as_ref().map(|n| n.as_bstr().to_string());
        if let Dest::Local(full, _) = &dest
            && current.as_deref() == Some(full.as_str())
        {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                format!("You are on {target} already."),
            )));
        }
        match &dest {
            Dest::Local(..) => p
                .steps
                .push(format!("Switches to {target}; your files show its state.")),
            Dest::Follows(full, _) => p.steps.push(format!(
                "Switches to your branch {} (it follows {target}).",
                full.trim_start_matches("refs/heads/")
            )),
            Dest::Track { name, id, .. } => {
                p.steps.push(format!(
                    "Creates the branch {name} from {target}, linked to it, and switches to it."
                ));
                p.refs.push(GitRefMove {
                    name: branch_full(name),
                    before: None,
                    after: Some(id.to_string()),
                });
            }
            Dest::Detached(id) => {
                p.steps.push(format!(
                    "Shows the files of {target} ({}). You will not be on a branch: create one there to keep new commits.",
                    short(id)
                ));
                p.warnings.push(format!("{target} is not a branch."));
            }
        }
        let id = dest.id();
        p.refs.push(GitRefMove {
            name: "HEAD".into(),
            before: cx.head.map(|h| h.to_string()),
            after: Some(id.to_string()),
        });
        let (from, to) = (self.flat_of(cx.head)?, self.flat_of(Some(id))?);
        p.files = diff_paths(&from, &to);
        p.backup = true;
        p.stop(cx.unsaved());
        p.stop(cx.in_progress()?);
        p.stop(self.would_overwrite(cx, &from, &to)?);
        Ok(p)
    }

    pub(crate) fn run_switch(&self, cx: &Ctx, run: &mut Run<'_>, target: &str) -> Result<Done> {
        let dest = self
            .switch_dest(target)?
            .ok_or_else(|| fail(format!("There is no branch, tag or commit \"{target}\".")))?;
        run.backup(self, "HEAD", cx.head)?;
        self.checkout_tree(cx, dest.id(), "switch branches")?;
        let full = |s: &str| FullName::try_from(s).map_err(|_| fail("invalid branch name"));
        match dest {
            Dest::Local(name, _) | Dest::Follows(name, _) => {
                let short = name.trim_start_matches("refs/heads/").to_owned();
                self.set_head(&full(&name)?, &format!("checkout: moving to {short}"))?;
                Ok(Done::new(format!("Switched to {short}."), true))
            }
            Dest::Track { remote, name, id } => {
                let branch = branch_full(&name);
                self.put_ref(
                    &branch,
                    Some(id),
                    None,
                    &format!("branch: Created from {target}"),
                )?;
                self.set_local_config(&[
                    ("branch", &name, "remote", &remote),
                    ("branch", &name, "merge", &branch),
                ])?;
                self.set_head(&full(&branch)?, &format!("checkout: moving to {name}"))?;
                Ok(Done::new(format!("Switched to {name}."), true))
            }
            Dest::Detached(id) => {
                self.detach_head(id, &format!("checkout: moving to {target}"))?;
                Ok(Done::new(
                    format!("Showing {target}. You are not on a branch."),
                    true,
                ))
            }
        }
    }

    // ---- rename-branch -----------------------------------------------------------------

    pub(crate) fn plan_rename_branch(
        &self,
        _cx: &Ctx,
        mut p: GitPlan,
        from: &str,
        to: &str,
    ) -> Result<GitPlan> {
        let at = if check_ref_name(from).is_ok() {
            self.ref_value(&branch_full(from))?
        } else {
            None
        };
        p.steps.push(format!("Renames {from} to {to}."));
        p.refs.push(GitRefMove {
            name: branch_full(from),
            before: at.map(|id| id.to_string()),
            after: None,
        });
        p.refs.push(GitRefMove {
            name: branch_full(to),
            before: None,
            after: at.map(|id| id.to_string()),
        });
        if self.config(&format!("branch.{from}.merge")).is_some() {
            p.warnings.push(format!(
                "The server keeps {from}; publish {to} to put the new name there."
            ));
        }
        if at.is_none() {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no branch {from}."),
            )));
        }
        p.stop(check_ref_name(to).err());
        if check_ref_name(to).is_ok() && self.ref_value(&branch_full(to))?.is_some() {
            p.stop(Some(block(
                GitBlockReason::Exists,
                format!("A branch named {to} exists already."),
            )));
        }
        Ok(p)
    }

    pub(crate) fn run_rename_branch(&self, cx: &Ctx, from: &str, to: &str) -> Result<Done> {
        let (old, new) = (branch_full(from), branch_full(to));
        let at = self
            .ref_value(&old)?
            .ok_or_else(|| fail(format!("There is no branch {from}.")))?;
        let message = format!("Branch: renamed {old} to {new}");
        self.put_ref(&new, Some(at), None, &message)?;
        if cx.head_ref.as_ref().is_some_and(|h| h.as_bstr() == old) {
            let full = FullName::try_from(new.as_str()).map_err(|_| fail("invalid branch name"))?;
            self.set_head(&full, &message)?;
        }
        self.put_ref(&old, None, Some(at), &message)?;
        // The upstream stays with the old name on the server (publish the new one).
        self.remove_config_section("branch", from)?;
        Ok(Done::new(format!("Renamed {from} to {to}."), false))
    }

    // ---- delete-branch -----------------------------------------------------------------

    /// Commits reachable from `tip` and from no other branch, remote branch, tag or `HEAD`.
    pub(crate) fn only_on(&self, tip: ObjectId, except: &str) -> Result<usize> {
        let mut hidden = Vec::new();
        let refs = self.repo.references().ctx("cannot list branches")?;
        for r in refs.all().ctx("cannot list branches")? {
            let mut r = r.map_err(|e| fail(format!("cannot list branches: {e}")))?;
            let name = r.name().as_bstr().to_str_lossy().into_owned();
            let counts = (name.starts_with("refs/heads/") && name != except)
                || name.starts_with("refs/remotes/")
                || name.starts_with("refs/tags/");
            if !counts {
                continue;
            }
            if let Ok(commit) = r.peel_to_commit() {
                hidden.push(commit.id);
            }
        }
        if let Some(head) = self.head_commit()?
            && self.head_ref()?.is_none_or(|h| h.as_bstr() != except)
        {
            hidden.push(head);
        }
        self.count_hidden(tip, &hidden)
    }

    pub(crate) fn plan_delete_branch(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        name: &str,
        force: bool,
    ) -> Result<GitPlan> {
        let full = branch_full(name);
        let at = if check_ref_name(name).is_ok() {
            self.ref_value(&full)?
        } else {
            None
        };
        let Some(at) = at else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no branch {name}."),
            )));
        };
        if cx.head_ref.as_ref().is_some_and(|h| h.as_bstr() == full) {
            return Ok(p.blocked_by(block(
                GitBlockReason::CurrentBranch,
                format!("You are on {name}. Switch to another branch before you delete it."),
            )));
        }
        let only = self.only_on(at, &full)?;
        let tracked = self.config(&format!("branch.{name}.merge")).is_some();
        p.steps.push(format!(
            "Deletes the branch {name} on this computer{}.",
            if tracked {
                " (the server keeps its copy)"
            } else {
                ""
            }
        ));
        p.refs.push(GitRefMove {
            name: full,
            before: Some(at.to_string()),
            after: None,
        });
        p.backup = true;
        if only > 0 {
            p.destructive = true;
            p.warnings.push(format!(
                "{} only on {name}. The backup keeps {} for Undo.",
                plural(only, "commit is", "commits are"),
                if only == 1 { "it" } else { "them" }
            ));
            if !force {
                p.stop(Some(block(
                    GitBlockReason::NotMerged,
                    format!(
                        "{name} has {} that {} on no other branch. Confirm to delete it anyway.",
                        plural_s(only, "commit"),
                        if only == 1 { "is" } else { "are" }
                    ),
                )));
            }
        }
        Ok(p)
    }

    pub(crate) fn run_delete_branch(&self, run: &mut Run<'_>, name: &str) -> Result<Done> {
        let full = branch_full(name);
        let at = self.ref_value(&full)?;
        run.backup(self, name, at)?;
        self.put_ref(&full, None, at, &format!("branch: deleted {name}"))?;
        self.remove_config_section("branch", name)?;
        Ok(Done::new(format!("Deleted {name}."), false))
    }

    // ---- merge -------------------------------------------------------------------------

    pub(crate) fn plan_merge(&self, cx: &Ctx, mut p: GitPlan, from: &str) -> Result<GitPlan> {
        p.stop(cx.need_branch());
        let Some(theirs) = self.try_resolve(from) else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no branch, tag or commit \"{from}\"."),
            )));
        };
        let (Some(head), Some(cur)) = (cx.head, cx.cur.as_deref()) else {
            return Ok(p);
        };
        if self.is_ancestor(theirs, head) {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                format!("{cur} already contains everything from {from}."),
            )));
        }
        let ff = self.is_ancestor(head, theirs);
        let incoming = self.commits_between(theirs, &[head], 50)?;
        let n_in = self.count_hidden(theirs, &[head])?;
        let files = if ff {
            let (a, b) = (self.flat_of(Some(head))?, self.flat_of(Some(theirs))?);
            diff_paths(&a, &b)
                .into_iter()
                .map(|path| GitMergeFile {
                    semantic: self.merges_on_model(&[&b], path.as_bytes()[1..].as_bstr()),
                    path,
                    clean: 1,
                    conflicts: Vec::new(),
                })
                .collect()
        } else {
            let tw = ThreeWay {
                kind: MergeKind::Merge,
                base: None,
                ours: head,
                theirs,
                label: from,
            };
            let mem = self.repo.clone().with_object_memory();
            let computed = self.compute_merge(&mem, &tw)?;
            self.merge_preview(computed)?
        };
        let conflicts = files.iter().filter(|f| !f.conflicts.is_empty()).count();
        p.steps.push(if ff {
            format!(
                "Moves {cur} forward to {from}: {} come in, no merge commit needed.",
                plural_s(n_in, "commit")
            )
        } else {
            format!(
                "Brings {} from {from} into {cur} with a merge commit.",
                plural_s(n_in, "commit")
            )
        });
        if conflicts > 0 {
            p.steps.push(format!(
                "{} your decision afterwards: the same fields were changed on both sides.",
                plural(conflicts, "file needs", "files need")
            ));
        }
        p.refs.push(GitRefMove {
            name: branch_full(cur),
            before: Some(head.to_string()),
            after: Some(if ff {
                theirs.to_string()
            } else {
                String::new()
            }),
        });
        p.files = files.iter().map(|f| f.path.clone()).collect();
        p.merge = Some(GitMergePreview {
            fast_forward: ff,
            commits: incoming,
            files,
        });
        p.backup = true;
        p.stop(cx.unsaved());
        p.stop(cx.in_progress()?);
        p.stop(cx.local_changes("merge")?);
        Ok(p)
    }

    pub(crate) fn run_merge(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        plan: &GitPlan,
        from: &str,
    ) -> Result<Done> {
        let (head, cur) = match (cx.head, cx.cur.clone()) {
            (Some(h), Some(c)) => (h, c),
            _ => return Err(fail("You are not on a branch.")),
        };
        let theirs = self.resolve_rev(from)?;
        run.backup(self, &cur, Some(head))?;
        let ff = plan.merge.as_ref().is_some_and(|m| m.fast_forward);
        if ff {
            self.checkout_tree(cx, theirs, "merge")?;
            self.put_ref(
                &branch_full(&cur),
                Some(theirs),
                Some(head),
                &format!("merge {from}: Fast-forward"),
            )?;
            let commits = u32::try_from(self.count_hidden(theirs, &[head])?).unwrap_or(u32::MAX);
            return Ok(Done {
                message: format!("Moved {cur} forward to {from}."),
                outcome: Some(GitPullResult::FastForward { commits }),
                reload: true,
            });
        }
        let identity = self.identity_or(run.opts.identity)?;
        let tw = ThreeWay {
            kind: MergeKind::Merge,
            base: None,
            ours: head,
            theirs,
            label: from,
        };
        let computed = self.compute_merge(&self.repo, &tw)?;
        let message = format!("Merge {from} into {cur}");
        let outcome = self.apply_merge(computed, &tw, &identity, None, &message, "merge")?;
        let message = match &outcome {
            GitPullResult::Conflicts { paths } => format!(
                "Merging {from} needs your decision in {}.",
                plural_s(paths.len(), "file")
            ),
            _ => format!("Merged {from} into {cur}."),
        };
        Ok(Done {
            message,
            outcome: Some(outcome),
            reload: true,
        })
    }

    // ---- pull, push ----------------------------------------------------------------------

    pub(crate) fn plan_pull(&self, cx: &Ctx, mut p: GitPlan) -> Result<GitPlan> {
        p.stop(cx.need_branch());
        p.network = true;
        p.backup = true;
        let up = self.upstream()?;
        if let Some(cur) = &cx.cur
            && up.is_none()
        {
            p.stop(Some(block(
                GitBlockReason::NoUpstream,
                format!("{cur} is not on the server yet. Publish it first."),
            )));
        }
        p.steps.push(format!(
            "Gets new commits from {} and merges them into {}.",
            up.as_ref()
                .map(|u| u.display.as_str())
                .unwrap_or("the server"),
            cx.cur.as_deref().unwrap_or("your branch")
        ));
        if let (Some(cur), Some(head)) = (&cx.cur, cx.head) {
            p.refs.push(GitRefMove {
                name: branch_full(cur),
                before: Some(head.to_string()),
                after: Some(String::new()),
            });
        }
        p.stop(cx.unsaved());
        p.stop(cx.in_progress()?);
        Ok(p)
    }

    pub(crate) fn run_pull(&self, cx: &Ctx, run: &mut Run<'_>) -> Result<Done> {
        if let Some(cur) = &cx.cur {
            run.backup(self, cur, cx.head)?;
        }
        let upstream = self
            .upstream()?
            .map(|u| u.display)
            .unwrap_or_else(|| "the server".into());
        let r = self.pull(run.opts.credentials, run.opts.identity)?;
        let message = match &r {
            GitPullResult::UpToDate => "Already up to date.".to_owned(),
            GitPullResult::FastForward { commits } => format!(
                "Got {} from {upstream}.",
                plural_s(*commits as usize, "commit")
            ),
            GitPullResult::Merged { .. } => format!("Merged {upstream}."),
            GitPullResult::Conflicts { paths } => format!(
                "Conflicts in {}: choose a side for each, then commit.",
                plural_s(paths.len(), "file")
            ),
        };
        let reload = r != GitPullResult::UpToDate;
        Ok(Done {
            message,
            outcome: Some(r),
            reload,
        })
    }

    pub(crate) fn plan_push(&self, cx: &Ctx, mut p: GitPlan) -> Result<GitPlan> {
        p.stop(cx.need_branch());
        p.network = true;
        p.undoable = false;
        let up = self.upstream()?;
        if let Some(cur) = &cx.cur
            && up.is_none()
        {
            p.stop(Some(block(
                GitBlockReason::NoUpstream,
                format!("{cur} is not on the server yet. Publish it first."),
            )));
        }
        if let (Some(_), Some(up), Some(head)) = (&cx.cur, &up, cx.head) {
            let theirs = self.ref_commit(&up.tracking)?;
            let (ahead, behind) = self.ahead_behind(&up.tracking)?;
            p.steps.push(format!(
                "Sends {} to {}. Sending cannot be undone from the app.",
                plural_s(ahead as usize, "commit"),
                up.display
            ));
            p.refs.push(GitRefMove {
                name: up.tracking.clone(),
                before: theirs.map(|id| id.to_string()),
                after: Some(head.to_string()),
            });
            if behind > 0 && ahead > 0 {
                p.stop(Some(block(
                    GitBlockReason::Diverged,
                    format!(
                        "{} has {} you do not have. Bring both together first.",
                        up.display,
                        plural_s(behind as usize, "commit")
                    ),
                )));
            } else if behind > 0 {
                p.stop(Some(block(
                    GitBlockReason::Rejected,
                    format!(
                        "{} has {}. Pull first, then push.",
                        up.display,
                        plural_s(behind as usize, "new commit")
                    ),
                )));
            } else if ahead == 0 {
                p.stop(Some(block(
                    GitBlockReason::NothingToDo,
                    "Nothing to send: the server has all your commits.",
                )));
            }
        }
        p.stop(cx.in_progress()?);
        Ok(p)
    }

    pub(crate) fn run_push(&self, cx: &Ctx, run: &mut Run<'_>) -> Result<Done> {
        let up = self.upstream()?.map(|u| u.display).unwrap_or_default();
        self.push(run.opts.credentials)?;
        Ok(Done::new(
            format!("Sent {} to {up}.", cx.cur.as_deref().unwrap_or("HEAD")),
            false,
        ))
    }

    // ---- reset ---------------------------------------------------------------------------

    /// Files whose working-tree content differs from `target` (untracked files only when
    /// `target` has them: Git leaves the others alone).
    fn differs_from(&self, cx: &Ctx, target: &Flat) -> Result<Vec<String>> {
        let head = self.flat_of(cx.head)?;
        let mut candidates: Vec<BString> = diff_paths(&head, target)
            .into_iter()
            .map(|p| BString::from(&p[1..]))
            .collect();
        for f in cx.changes()? {
            let path = BString::from(f.path.as_str());
            if f.state != GitFileState::Untracked || target.contains_key(&path) {
                candidates.push(path);
            }
        }
        candidates.sort();
        candidates.dedup();
        let index = cx.whole.load_index()?;
        let mut probe = Probe::new(&cx.whole)?;
        let mut out = Vec::new();
        for p in candidates {
            let now = match probe.state(p.as_bstr(), &index)? {
                FileState::File(b) => Some(b),
                _ => None,
            };
            let want = target.get(&p).copied();
            let same = match (now, want) {
                (Some(a), Some(b)) => crate::worktree::same(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                out.push(slash(&p));
            }
        }
        Ok(out)
    }

    pub(crate) fn plan_reset(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        target: &str,
        mode: GitResetMode,
    ) -> Result<GitPlan> {
        p.stop(cx.need_branch());
        let Some(to) = self.try_resolve(target) else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no commit \"{target}\"."),
            )));
        };
        let (Some(head), Some(cur)) = (cx.head, cx.cur.as_deref()) else {
            return Ok(p);
        };
        if to == head && mode == GitResetMode::KeepChanges {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                format!("{cur} is there already."),
            )));
        }
        let leaving = self.count_hidden(head, &[to])?;
        p.steps.push(format!("Moves {cur} to {}.", self.named(to)));
        p.steps.push(match mode {
            GitResetMode::KeepChanges => {
                "Your files stay as they are; what the removed commits did shows as changes."
            }
            GitResetMode::Discard => {
                "Your files match that commit; uncommitted changes are thrown away (the backup keeps them)."
            }
        }
        .into());
        if leaving > 0 {
            p.warnings.push(format!(
                "{} {cur}. The backup keeps {} for Undo.",
                plural(leaving, "commit leaves", "commits leave"),
                if leaving == 1 { "it" } else { "them" }
            ));
            if let Some(up) = self.upstream()?
                && let Some(theirs) = self.ref_commit(&up.tracking)?
            {
                // Commits that leave the branch but are on the server already.
                let leave_but_pushed =
                    self.count_hidden(head, &[to])? - self.count_hidden(head, &[to, theirs])?;
                if leave_but_pushed > 0 {
                    p.warnings.push(
                        "Some of these commits are on the server already. The next Pull brings them back."
                            .into(),
                    );
                }
            }
        }
        p.refs.push(GitRefMove {
            name: branch_full(cur),
            before: Some(head.to_string()),
            after: Some(to.to_string()),
        });
        p.backup = true;
        if mode == GitResetMode::Discard {
            p.destructive = true;
            p.files = self.differs_from(cx, &self.flat_of(Some(to))?)?;
            p.stop(cx.unsaved());
        }
        p.stop(cx.in_progress()?);
        Ok(p)
    }

    pub(crate) fn run_reset(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        target: &str,
        mode: GitResetMode,
    ) -> Result<Done> {
        let (head, cur) = match (cx.head, cx.cur.clone()) {
            (Some(h), Some(c)) => (h, c),
            _ => return Err(fail("You are not on a branch.")),
        };
        let to = self.resolve_rev(target)?;
        run.backup(self, &cur, Some(head))?;
        let branch = branch_full(&cur);
        let message = format!("reset: moving to {target}");
        let to_tree = self.commit_tree(to)?;
        match mode {
            GitResetMode::KeepChanges => {
                self.put_ref(&branch, Some(to), Some(head), &message)?;
                let old = cx.whole.load_index()?;
                let mut index = cx.whole.reindex(to_tree, &[&old])?;
                cx.whole.write_index(&mut index)?;
                Ok(Done::new(format!("Moved {cur} to {}.", short(&to)), false))
            }
            GitResetMode::Discard => {
                let now = crate::undo::snapshot_tree(&cx.whole)?;
                // The commit's files, and the untracked files it does not have.
                let now_flat = flatten(&self.repo, now)?;
                let to_flat = flatten(&self.repo, to_tree)?;
                let mut editor = self.repo.edit_tree(to_tree).ctx("cannot build the tree")?;
                for f in cx.changes()? {
                    let path = BString::from(f.path.as_str());
                    if f.state == GitFileState::Untracked
                        && !to_flat.contains_key(&path)
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
                self.put_ref(&branch, Some(to), Some(head), &message)?;
                if let Err(e) = cx.whole.move_worktree(now, want, "reset") {
                    let _ = self.put_ref(&branch, Some(head), Some(to), "reset: undo");
                    return Err(e);
                }
                Ok(Done::new(format!("Moved {cur} to {}.", short(&to)), true))
            }
        }
    }

    // ---- discard -------------------------------------------------------------------------

    fn discard_hits(&self, cx: &Ctx, paths: &[String]) -> Result<Vec<(String, GitFileState)>> {
        // A leading `/`: relative to the repository; otherwise relative to the scope
        // (the Git panel lists the scope's files).
        let wanted = paths
            .iter()
            .map(|p| match p.strip_prefix('/') {
                Some(rest) => Ok(rest.to_owned()),
                None => self.scope.abs(p).map(|b| b.to_string()),
            })
            .collect::<Result<Vec<String>>>()?;
        Ok(cx
            .changes()?
            .iter()
            .filter(|f| wanted.is_empty() || wanted.contains(&f.path))
            .map(|f| (f.path.clone(), f.state))
            .collect())
    }

    pub(crate) fn plan_discard(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        paths: &[String],
    ) -> Result<GitPlan> {
        let hits = self.discard_hits(cx, paths)?;
        if hits.is_empty() {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                "There are no changes to discard.",
            )));
        }
        p.steps.push(format!(
            "Throws away the changes to {}; they go back to the last commit.",
            plural_s(hits.len(), "file")
        ));
        p.steps
            .push("A backup keeps them, so Undo can bring them back.".into());
        p.files = hits.iter().map(|(f, _)| format!("/{f}")).collect();
        p.backup = true;
        p.destructive = true;
        p.stop(cx.unsaved());
        if hits.iter().any(|(_, s)| *s == GitFileState::Conflicted) {
            p.stop(Some(block(
                GitBlockReason::InProgress,
                "Resolve the conflicts first.",
            )));
        }
        Ok(p)
    }

    pub(crate) fn run_discard(&self, cx: &Ctx, _run: &mut Run<'_>, plan: &GitPlan) -> Result<Done> {
        let now = crate::undo::snapshot_tree(&cx.whole)?;
        let head = self.flat_of(cx.head)?;
        let mut editor = self.repo.edit_tree(now).ctx("cannot build the tree")?;
        for f in &plan.files {
            let path = BString::from(&f[1..]);
            match head.get(&path) {
                Some(blob) => editor
                    .upsert(
                        path.as_bstr(),
                        crate::commit::entry_kind(blob.mode),
                        blob.id,
                    )
                    .ctx("cannot build the tree")?,
                None => editor.remove(path.as_bstr()).ctx("cannot build the tree")?,
            };
        }
        let want = editor.write().ctx("cannot write the tree")?.detach();
        cx.whole.move_worktree(now, want, "discard")?;
        Ok(Done::new(
            format!(
                "Discarded the changes to {}.",
                plural_s(plan.files.len(), "file")
            ),
            true,
        ))
    }
}

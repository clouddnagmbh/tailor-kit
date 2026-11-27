// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! GitTailor wave 3, strand B: [`GitAction::ForcePush`], a force push that is always a lease,
//! and the protected branches of a product ([`Repo::with_protected`]).
//!
//! A force push replaces the branch on the remote only
//! - while the remote branch still points at `expect`, the tip the user saw in the plan
//!   (`git push --force-with-lease=<ref>:<expect>`; `expect` empty: the branch must not exist
//!   there yet). The update command on the wire carries `expect` as the old value, so the
//!   remote itself refuses it when someone pushed in between; nothing is sent then;
//! - when `expect` was integrated locally (`--force-if-includes`): the local branch, or one of
//!   the entries of its reflog, contains it ([`Repo::integrated`]). A
//!   background fetch that brought someone else's commits into the remote-tracking branch
//!   therefore cannot be overwritten by accident: the plan blocks with
//!   [`GitBlockReason::Rejected`].
//!
//! Only for products that opt in ([`Repo::with_force_push`]); never on a protected branch
//! ([`GitBlockReason::Protected`]). Before anything is sent, a backup ref
//! `refs/<namespace>/backup/<unix>/remote-<remote>-<branch>` keeps the overwritten remote tip.
//! With [`Repo::with_push_undo`] the journal entry says what moved on the remote, and undo puts
//! it back with a lease against the pushed value (refused once someone pushed on top).

use gix::ObjectId;
use gix::refs::transaction::PreviousValue;
use tk_git_types::GitCommit;
use tk_git_types::actions::{GitBlockReason, GitPlan, GitRefMove};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural_s, short};
use crate::remote::{Pushed, Upstream};
use crate::repo::Repo;
use crate::util::{Context, Result, fail};

/// Commits shown at most in a plan's list of what the remote loses.
pub const OVERWRITTEN_LIMIT: usize = 200;

/// Where a force push of a branch goes, as far as the repository knows without the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForcePushTarget {
    /// Short name of the local branch.
    pub branch: String,
    pub remote: String,
    /// The branch on the remote (`refs/heads/<name>`).
    pub remote_ref: String,
    /// The remote-tracking ref (`refs/remotes/<remote>/<name>`).
    pub tracking: String,
    /// What the local branch points to.
    pub local: Option<String>,
    /// What the remote-tracking ref points to: the remote tip as last fetched, the lease a plan
    /// offers (`None`: not on the remote as far as known).
    pub tracked: Option<String>,
}

/// `*` stands for any run of characters without `/`; everything else is literal.
pub fn protected_glob(pattern: &str, name: &str) -> bool {
    fn go(p: &[u8], n: &[u8]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some((b'*', rest)) => {
                let mut i = 0;
                loop {
                    if go(rest, &n[i..]) {
                        return true;
                    }
                    if i == n.len() || n[i] == b'/' {
                        return false;
                    }
                    i += 1;
                }
            }
            Some((c, rest)) => n.first() == Some(c) && go(rest, &n[1..]),
        }
    }
    go(pattern.as_bytes(), name.as_bytes())
}

fn parse_id(hex: &str, what: &str) -> Result<ObjectId> {
    ObjectId::from_hex(hex.trim().as_bytes())
        .map_err(|_| fail(format!("{what} is not a commit id")))
}

impl Repo {
    /// The pattern of [`Repo::with_protected`] that protects branch `branch` (short name), if
    /// any: a force push there plans blocked, and so does a rewrite of its published commits.
    pub fn protected_pattern(&self, branch: &str) -> Option<String> {
        self.protected
            .iter()
            .find(|p| protected_glob(p, branch))
            .cloned()
    }

    /// The upstream of local branch `branch` (`branch.<name>.remote`/`.merge`); `None` without
    /// one or when it is the repository itself (`.`).
    fn force_push_upstream(&self, branch: &str) -> Option<Upstream> {
        let remote = self.config(&format!("branch.{branch}.remote"))?;
        let merge = self.config(&format!("branch.{branch}.merge"))?;
        if remote == "." {
            return None;
        }
        let short = merge.strip_prefix("refs/heads/")?;
        Some(Upstream {
            tracking: format!("refs/remotes/{remote}/{short}"),
            display: format!("{remote}/{short}"),
            remote,
            merge,
        })
    }

    /// Where a force push of `branch` (`None`: the current one) goes; `None` when the branch
    /// does not exist or has no upstream.
    pub fn force_push_target(&self, branch: Option<&str>) -> Result<Option<ForcePushTarget>> {
        let branch = match branch {
            Some(b) => b.to_owned(),
            None => match self.head_branch()? {
                Some(b) => b,
                None => return Ok(None),
            },
        };
        let Some(up) = self.force_push_upstream(&branch) else {
            return Ok(None);
        };
        let local = self.ref_commit(&format!("refs/heads/{branch}"))?;
        let tracked = self.ref_commit(&up.tracking)?;
        Ok(Some(ForcePushTarget {
            branch,
            remote: up.remote,
            remote_ref: up.merge,
            tracking: up.tracking,
            local: local.map(|id| id.to_string()),
            tracked: tracked.map(|id| id.to_string()),
        }))
    }

    /// Commits a force push of `local` over `expect` takes off the remote (reachable from
    /// `expect`, not from `local`), newest first, at most [`OVERWRITTEN_LIMIT`].
    pub fn overwritten_by(&self, local: &str, expect: &str) -> Result<Vec<GitCommit>> {
        let local = parse_id(local, "the local tip")?;
        let expect = parse_id(expect, "the expected remote tip")?;
        if !self.repo.has_object(expect) {
            return Ok(Vec::new());
        }
        self.commits_between(expect, &[local], OVERWRITTEN_LIMIT)
    }

    /// `--force-if-includes` for local branch `branch`: whether `commit` was integrated
    /// locally: the branch contains it, or one of the entries of the branch's reflog does. What a
    /// fetch brought that was never in the branch is not. (Git stops looking at the first entry
    /// older than the newest update of the remote-tracking ref; this looks at the whole reflog,
    /// because a product's own undo of a push moves the remote-tracking ref too, and the commit
    /// it puts back was made here. Commits never seen here are refused either way.)
    pub fn integrated(&self, branch: &str, commit: &str) -> Result<bool> {
        let commit = parse_id(commit, "the expected remote tip")?;
        if !self.repo.has_object(commit) {
            return Ok(false);
        }
        let name = format!("refs/heads/{branch}");
        if let Some(tip) = self.ref_commit(&name)?
            && (tip == commit || self.is_ancestor(commit, tip))
        {
            return Ok(true);
        }
        let Some(reference) = self
            .repo
            .try_find_reference(name.as_str())
            .ctx("cannot read the reflog")?
        else {
            return Ok(false);
        };
        let mut platform = reference.log_iter();
        let Some(lines) = platform.rev().ctx("cannot read the reflog")? else {
            return Ok(false);
        };
        for line in lines {
            let id = line.ctx("cannot read the reflog")?.new_oid;
            if id == commit || (self.repo.has_object(id) && self.is_ancestor(commit, id)) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn plan_force_push(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        branch: Option<&str>,
        expect: &str,
    ) -> Result<GitPlan> {
        p.network = true;
        p.undoable = self.push_undo;
        if !self.force_push {
            return Ok(p.blocked_by(block(
                GitBlockReason::Unsupported,
                "Force push is not available in this app.",
            )));
        }
        let name = match branch {
            Some(b) => b.to_owned(),
            None => match &cx.cur {
                Some(c) => c.clone(),
                None => {
                    return Ok(p.blocked_by(cx.need_branch().unwrap_or_else(|| {
                        block(GitBlockReason::Detached, "Switch to a branch first.")
                    })));
                }
            },
        };
        let Some(target) = self.force_push_target(Some(&name))? else {
            if self.ref_commit(&format!("refs/heads/{name}"))?.is_none() {
                return Ok(p.blocked_by(block(
                    GitBlockReason::NotFound,
                    format!("There is no branch {name}."),
                )));
            }
            return Ok(p.blocked_by(block(
                GitBlockReason::NoUpstream,
                format!("{name} is not on the server yet. Publish it first."),
            )));
        };
        let Some(local) = target.local.clone() else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no branch {name}."),
            )));
        };
        let shown = target
            .remote_ref
            .strip_prefix("refs/heads/")
            .map(|b| format!("{}/{b}", target.remote))
            .unwrap_or_else(|| target.remote_ref.clone());
        if let Some(pattern) = self.protected_pattern(&name) {
            p.stop(Some(block(
                GitBlockReason::Protected,
                format!(
                    "{name} is protected ({pattern}): it is never force pushed. Bring both sides together with a merge instead."
                ),
            )));
        }
        let expect = expect.trim();
        let expected = if expect.is_empty() {
            None
        } else {
            match ObjectId::from_hex(expect.as_bytes()) {
                Ok(id) => Some(id),
                Err(_) => {
                    return Ok(p.blocked_by(block(
                        GitBlockReason::NotFound,
                        format!("\"{expect}\" is not a commit id."),
                    )));
                }
            }
        };
        let local_id = parse_id(&local, "the local tip")?;
        let overwritten = match expected {
            Some(e) => self.overwritten_by(&local, &e.to_string())?,
            None => Vec::new(),
        };
        p.steps.push(match expected {
            Some(e) if overwritten.is_empty() => format!(
                "Sends {name} to {shown}, only while {shown} still is {}.",
                short(&e)
            ),
            Some(e) => format!(
                "Replaces {shown} with your {name}, only while it still is {}: {} there {} away (a backup keeps them).",
                short(&e),
                plural_s(overwritten.len(), "commit"),
                if overwritten.len() == 1 { "goes" } else { "go" }
            ),
            None => format!("Sends {name} to {shown}, only while {shown} does not exist there yet."),
        });
        p.steps.push(if self.push_undo {
            format!("Undo puts {shown} back where it was, as long as nobody pushed on top.")
        } else {
            "Sending cannot be undone from the app.".into()
        });
        p.refs.push(GitRefMove {
            name: target.tracking.clone(),
            before: target.tracked.clone(),
            after: Some(local.clone()),
        });
        p.backup = expected.is_some();
        if let Some(e) = expected {
            if e == local_id {
                p.stop(Some(block(
                    GitBlockReason::NothingToDo,
                    "Nothing to send: the server has all your commits.",
                )));
            } else if !self.integrated(&name, &e.to_string())? {
                let theirs = if self.repo.has_object(e) {
                    self.count_hidden(e, &[local_id])?
                } else {
                    0
                };
                p.stop(Some(block(
                    GitBlockReason::Rejected,
                    if theirs > 0 {
                        format!(
                            "Someone pushed to {shown} in the meantime: {} there never were in your {name}. Get them first (pull or rebase), then push again.",
                            plural_s(theirs, "commit")
                        )
                    } else {
                        format!(
                            "Someone pushed to {shown} in the meantime: fetch and look at their commits, then push again."
                        )
                    },
                )));
            }
        }
        if target.tracked.as_deref() != expected.map(|e| e.to_string()).as_deref() {
            p.warnings.push(format!(
                "{shown} moved since this plan was made: the push is refused unless the server still has what you saw."
            ));
        }
        p.stop(cx.in_progress()?);
        Ok(p)
    }

    pub(crate) fn run_force_push(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        branch: Option<&str>,
        expect: &str,
    ) -> Result<Done> {
        let name = match branch {
            Some(b) => b.to_owned(),
            None => cx
                .cur
                .clone()
                .ok_or_else(|| fail("not on a branch: switch to one before pushing"))?,
        };
        let target = self
            .force_push_target(Some(&name))?
            .ok_or_else(|| fail(format!("{name} has no upstream: publish it first")))?;
        let local = parse_id(
            target
                .local
                .as_deref()
                .ok_or_else(|| fail(format!("there is no branch {name}")))?,
            "the local tip",
        )?;
        let expect = expect.trim();
        let expected = if expect.is_empty() {
            None
        } else {
            Some(parse_id(expect, "the expected remote tip")?)
        };
        let shown = target
            .remote_ref
            .strip_prefix("refs/heads/")
            .map(|b| format!("{}/{b}", target.remote))
            .unwrap_or_else(|| target.remote_ref.clone());
        // The overwritten remote tip first, so nothing is lost when the push goes through.
        let backups_before = run.backups.len();
        if let Some(e) = expected
            && self.repo.has_object(e)
        {
            let what = format!(
                "remote-{}-{}",
                target.remote,
                target
                    .remote_ref
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&target.remote_ref)
            );
            run.backup(self, &what, Some(e))?;
        }
        let sent = self.update_remote_ref(
            &target.remote,
            Some(local),
            &target.remote_ref,
            Some(expected),
            run.opts.credentials,
        );
        if let Err(e) = sent {
            // Nothing was sent: the backup of this run is not needed.
            for name in run.backups.drain(backups_before..) {
                let _ = self.put_ref(&name, None, expected, "backup not needed");
            }
            // (`expected` is what the backup points to.)
            return Err(match e {
                crate::GitError::Failed(m) if m.contains("changed since") => fail(format!(
                    "{shown} changed since you looked: someone pushed in the meantime. Nothing was sent. Fetch, look at their commits, then plan again."
                )),
                other => other,
            });
        }
        self.repo
            .reference(
                target.tracking.as_str(),
                local,
                PreviousValue::Any,
                "update by push",
            )
            .ctx("cannot update the remote-tracking branch")?;
        if self.push_undo {
            run.pushed = Some(Pushed {
                remote: target.remote.clone(),
                remote_ref: target.remote_ref.clone(),
                before: expected.map(|e| e.to_string()),
                after: Some(local.to_string()),
            });
        }
        Ok(Done::new(
            match expected {
                Some(_) => {
                    format!("Replaced {shown} with your {name} (a backup keeps what was there).")
                }
                None => format!("Sent {name} to {shown}."),
            },
            false,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::protected_glob;

    #[test]
    fn globs_stop_at_slashes() {
        assert!(protected_glob("release/*", "release/1.0"));
        assert!(!protected_glob("release/*", "release/1/2"));
        assert!(protected_glob("main", "main"));
        assert!(!protected_glob("main", "mainline"));
        assert!(protected_glob("*", "feature"));
        assert!(!protected_glob("*", "a/b"));
        assert!(protected_glob("*/stable", "team/stable"));
        assert!(!protected_glob("*/stable", "a/b/stable"));
    }
}

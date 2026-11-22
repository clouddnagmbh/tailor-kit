// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Branches: list, create, switch.

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};
use rt_app::git::GitBranch;

use crate::repo::Repo;
use crate::util::{Context, Result, fail};
use crate::worktree::switch;

/// `refs/heads/<name>`, if `name` is a valid branch name.
pub fn branch_ref(name: &str) -> Result<FullName> {
    let bad = || fail(format!("invalid branch name: {name:?}"));
    if name.is_empty()
        || name.starts_with('-')
        || name == "HEAD"
        || name.contains(char::is_whitespace)
    {
        return Err(bad());
    }
    FullName::try_from(format!("refs/heads/{name}")).map_err(|_| bad())
}

impl Repo {
    /// Local branches (sorted), then remote-tracking branches (`origin/main`).
    pub fn branches(&self) -> Result<Vec<GitBranch>> {
        let current = self.head_ref()?;
        let refs = self.repo.references().ctx("cannot list branches")?;
        let mut local = Vec::new();
        for r in refs.local_branches().ctx("cannot list branches")? {
            let r = r.map_err(|e| fail(format!("cannot list branches: {e}")))?;
            local.push(GitBranch {
                name: r.name().shorten().to_str_lossy().into_owned(),
                current: current.as_ref().is_some_and(|c| c.as_ref() == r.name()),
                remote: false,
            });
        }
        // Without commits the current branch has no ref yet; list it anyway (as web does).
        if let Some(name) = self.head_branch()?
            && self.head_commit()?.is_none()
            && !local.iter().any(|b| b.name == name)
        {
            local.push(GitBranch {
                name,
                current: true,
                remote: false,
            });
        }
        let mut remote = Vec::new();
        for r in refs.remote_branches().ctx("cannot list branches")? {
            let r = r.map_err(|e| fail(format!("cannot list branches: {e}")))?;
            let name = r.name().shorten().to_str_lossy().into_owned();
            if name.ends_with("/HEAD") {
                continue;
            }
            remote.push(GitBranch {
                name,
                current: false,
                remote: true,
            });
        }
        local.sort_by(|a, b| a.name.cmp(&b.name));
        remote.sort_by(|a, b| a.name.cmp(&b.name));
        local.extend(remote);
        Ok(local)
    }

    /// Points `HEAD` at `branch` (a full ref name) without touching the working tree.
    pub(crate) fn set_head(&self, branch: &FullName, message: &str) -> Result<()> {
        self.repo
            .edit_reference(RefEdit {
                change: Change::Update {
                    log: LogChange {
                        mode: RefLog::AndReference,
                        force_create_reflog: false,
                        message: message.into(),
                    },
                    expected: PreviousValue::Any,
                    new: Target::Symbolic(branch.clone()),
                },
                name: "HEAD".try_into().expect("valid"),
                deref: false,
            })
            .ctx("cannot move HEAD")?;
        Ok(())
    }

    /// Creates or moves a local branch ref.
    pub(crate) fn set_branch(
        &self,
        branch: &FullName,
        id: ObjectId,
        expected: PreviousValue,
        message: &str,
    ) -> Result<()> {
        self.repo
            .reference(branch.clone(), id, expected, message)
            .ctx("cannot update the branch")?;
        Ok(())
    }

    /// Creates branch `name` at `HEAD`; with `checkout`, also switches to it (the working tree
    /// stays as it is: both point at the same commit).
    pub fn create_branch(&self, name: &str, checkout: bool) -> Result<()> {
        let full = branch_ref(name)?;
        let Some(head) = self.head_commit()? else {
            return Err(fail("commit something before creating a branch"));
        };
        if self
            .ref_commit(full.as_bstr().to_str_lossy().as_ref())?
            .is_some()
        {
            return Err(fail(format!("a branch named {name} already exists")));
        }
        self.set_branch(
            &full,
            head,
            PreviousValue::MustNotExist,
            "branch: Created from HEAD",
        )?;
        if checkout {
            self.set_head(&full, &format!("checkout: moving to {name}"))?;
        }
        Ok(())
    }

    /// Switches to the local branch `branch`, or to the remote-tracking branch `origin/x`
    /// (then through a local `x` that tracks it, created if needed). Refuses if files that
    /// differ between the branches have local changes.
    pub fn checkout(&self, branch: &str) -> Result<()> {
        if self.merge_head()?.is_some() {
            return Err(fail(
                "finish the merge (resolve and commit) before switching branches",
            ));
        }
        let local = branch_ref(branch)?;
        let local_name = local.as_bstr().to_str_lossy().into_owned();
        let (target, local, tracking) = if let Some(id) = self.ref_commit(&local_name)? {
            (id, local, None)
        } else if let Some(id) = self.ref_commit(&format!("refs/remotes/{branch}"))? {
            let (remote, name) = branch
                .split_once('/')
                .ok_or_else(|| fail(format!("no branch named {branch}")))?;
            let local = branch_ref(name)?;
            let local_name = local.as_bstr().to_str_lossy().into_owned();
            match self.ref_commit(&local_name)? {
                Some(existing) => (existing, local, None),
                None => (id, local, Some((remote.to_owned(), name.to_owned()))),
            }
        } else {
            return Err(fail(format!("no branch named {branch}")));
        };
        if self.head_ref()?.as_ref() == Some(&local) {
            return Ok(());
        }
        let from = self.head_tree()?;
        let to = self.commit_tree(target)?;
        let short = local.shorten().to_str_lossy().into_owned();
        let mut index = switch(self, from, to, "switch branches")?;
        self.write_index(&mut index)?;
        if let Some((remote, name)) = tracking {
            self.set_branch(
                &local,
                target,
                PreviousValue::MustNotExist,
                &format!("branch: Created from {branch}"),
            )?;
            let merge = format!("refs/heads/{name}");
            self.set_local_config(&[
                ("branch", &short, "remote", &remote),
                ("branch", &short, "merge", &merge),
            ])?;
        }
        self.set_head(&local, &format!("checkout: moving to {short}"))
    }
}

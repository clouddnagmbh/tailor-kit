// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitAction::Amend` (wave 15, strand D): replace the last commit of the current branch with
//! a new one that has the same parents and author, the message asked for and, optionally, the
//! current content of some files (or what the index holds). Refused when that commit is
//! already on a remote-tracking branch ([`GitBlockReason::Published`]): the app never rewrites
//! history others may have. The old commit stays reachable through the backup ref.
//!
//! The working files do not change. Commits are not signed here (signing arrives with the
//! index and signature hooks, strand F).

use gix::ObjectId;
use gix::bstr::ByteSlice;
use rt_app::git::GitFileState;
use rt_app::git::actions::{GitBlockReason, GitPlan, GitRefMove};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural_s};
use crate::branch_actions::{branch_full, diff_paths};
use crate::repo::Repo;
use crate::undo::tree_with;
use crate::util::{Context, Result, fail, flatten};

impl Repo {
    /// Remote-tracking branch tips.
    fn remote_tips(&self) -> Result<Vec<ObjectId>> {
        Ok(self.remote_refs()?.into_values().collect())
    }

    /// How many of the commits reachable from `tip` but not from `hidden` are also on a
    /// remote-tracking branch.
    pub(crate) fn published_in(&self, tip: ObjectId, hidden: &[ObjectId]) -> Result<usize> {
        let tips = self.remote_tips()?;
        if tips.is_empty() {
            return Ok(0);
        }
        let all = self.count_hidden(tip, hidden)?;
        let mut more = hidden.to_vec();
        more.extend(tips);
        Ok(all - self.count_hidden(tip, &more)?)
    }

    /// The tree the amended commit gets: `HEAD`'s with `paths` from the working files, or the
    /// index. `None`: the tree does not change.
    fn amend_tree(
        &self,
        cx: &Ctx,
        paths: &[String],
        staged: bool,
    ) -> Result<Option<(ObjectId, Vec<String>)>> {
        let head_tree = cx.whole.head_tree()?;
        if staged {
            let index = cx.whole.load_index()?;
            let tree = cx.whole.tree_from_index(&index)?;
            if tree == head_tree {
                return Ok(None);
            }
            let files = diff_paths(
                &flatten(&self.repo, head_tree)?,
                &flatten(&self.repo, tree)?,
            );
            return Ok(Some((tree, files)));
        }
        if paths.is_empty() {
            return Ok(None);
        }
        let sel = self.selection(paths)?;
        let files: Vec<_> = cx
            .changes()?
            .iter()
            .filter(|f| f.state != GitFileState::Conflicted && sel.matches(f.path.as_bytes()))
            .collect();
        if files.is_empty() {
            return Ok(None);
        }
        let list = files.iter().map(|f| format!("/{}", f.path)).collect();
        Ok(Some((tree_with(&cx.whole, head_tree, &files)?, list)))
    }

    pub(crate) fn plan_amend(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        message: Option<&str>,
        paths: &[String],
        staged: bool,
    ) -> Result<GitPlan> {
        p.stop(cx.need_branch());
        let (Some(head), Some(cur)) = (cx.head, cx.cur.as_deref()) else {
            return Ok(p);
        };
        p.stop(cx.in_progress()?);
        if !paths.is_empty() {
            p.stop(cx.unsaved());
        }
        let commit = self.repo.find_commit(head).ctx("cannot read a commit")?;
        let old_message = commit.decode().ctx("cannot read a commit")?.message.to_str_lossy().into_owned();
        if let Some(m) = message
            && m.trim().is_empty()
        {
            return Ok(p.blocked_by(block(
                GitBlockReason::InvalidName,
                "The commit message is empty.",
            )));
        }
        let parents: Vec<ObjectId> = commit.parent_ids().map(|i| i.detach()).collect();
        if self.published_in(head, &parents)? > 0 {
            return Ok(p.blocked_by(block(
                GitBlockReason::Published,
                "The last commit is already on the server, so changing it would rewrite history others may have. Make a new commit instead.",
            )));
        }
        let content = self.amend_tree(cx, paths, staged)?;
        let new_message = message.map(|m| m.trim()).filter(|m| *m != old_message.trim());
        if content.is_none() && new_message.is_none() {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                if paths.is_empty() && !staged {
                    "There is nothing to change: give a new message or choose files."
                } else {
                    "Nothing changed in these files since the last commit."
                },
            )));
        }
        let mut what = Vec::new();
        if let Some(m) = new_message {
            what.push(format!("the message \"{}\"", m.lines().next().unwrap_or("")));
        }
        if let Some((_, files)) = &content {
            what.push(format!("the current content of {}", plural_s(files.len(), "file")));
        }
        p.steps.push(format!(
            "Replaces the last commit {} on {cur} with a new one that has {}. Its author and place in history stay.",
            self.named(head),
            what.join(" and ")
        ));
        p.steps
            .push("Your files do not change. The backup keeps the old commit for Undo.".into());
        if let Some((_, files)) = content {
            p.files = files;
        }
        p.refs.push(GitRefMove {
            name: branch_full(cur),
            before: Some(head.to_string()),
            after: Some(String::new()),
        });
        p.backup = true;
        Ok(p)
    }

    pub(crate) fn run_amend(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        message: Option<&str>,
        paths: &[String],
        staged: bool,
    ) -> Result<Done> {
        let (head, cur) = match (cx.head, cx.cur.clone()) {
            (Some(h), Some(c)) => (h, c),
            _ => return Err(fail("You are not on a branch.")),
        };
        let commit = self.repo.find_commit(head).ctx("cannot read a commit")?;
        let decoded = commit.decode().ctx("cannot read a commit")?;
        let author: gix::actor::Signature = decoded
            .author()
            .ctx("cannot read a commit")?
            .to_owned()
            .ctx("cannot read a commit")?;
        let old_message = decoded.message.to_str_lossy().into_owned();
        let encoding = decoded.encoding.map(|e| e.to_owned());
        let parents: Vec<ObjectId> = commit.parent_ids().map(|i| i.detach()).collect();
        let content = self.amend_tree(cx, paths, staged)?;
        let tree = match &content {
            Some((tree, _)) => *tree,
            None => self.commit_tree(head)?,
        };
        let message = match message.map(str::trim) {
            Some(m) if !m.is_empty() => format!("{m}\n"),
            _ => old_message,
        };
        let identity = self.identity_or(run.opts.identity)?;
        let new = gix::objs::Commit {
            tree,
            parents: parents.into(),
            author,
            committer: crate::commit::signature(&identity),
            encoding,
            message: message.as_str().into(),
            extra_headers: Vec::new(),
        };
        let new = self
            .repo
            .write_object(&new)
            .ctx("cannot write the commit")?
            .detach();
        run.backup(self, &cur, Some(head))?;
        self.put_ref(
            &branch_full(&cur),
            Some(new),
            Some(head),
            &format!("commit (amend): {}", message.lines().next().unwrap_or("")),
        )?;
        if content.is_some() {
            let old = cx.whole.load_index()?;
            let mut index = cx.whole.reindex(tree, &[&old])?;
            cx.whole.write_index(&mut index)?;
        }
        Ok(Done::new(
            format!(
                "Replaced the last commit on {cur} with \"{}\".",
                message.lines().next().unwrap_or("")
            ),
            false,
        ))
    }
}

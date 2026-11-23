// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitAction::RestoreFiles` and `GitAction::RestoreBranch` (wave 15, strand D), and the path
//! selection the amend and partial-stash actions share.
//!
//! - Restore files: the working files named become what they are at a revision (a path
//!   missing there is deleted). Uncommitted work in them is thrown away, so the working files
//!   get a backup ref and undo brings them back.
//! - Restore branch: a branch that is not checked out (also a deleted one) points at a commit
//!   again, e.g. from the reflog or a backup ref.

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use rt_app::git::actions::{GitBlockReason, GitPlan, GitRefMove, check_ref_name};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural, plural_s, short};
use crate::branch_actions::branch_full;
use crate::repo::Repo;
use crate::util::{Context, Result, flatten};

/// Selects repository paths: a file, or a folder with everything under it. Empty: all.
pub(crate) struct Selection(Vec<BString>);

impl Selection {
    pub fn matches(&self, path: &[u8]) -> bool {
        self.0.is_empty()
            || self.0.iter().any(|p| {
                path == p.as_slice()
                    || (path.starts_with(p.as_slice()) && path.get(p.len()) == Some(&b'/'))
            })
    }
}

impl Repo {
    /// `paths` as the plans give them (repository-relative with a leading `/`; without one,
    /// relative to the collection).
    pub(crate) fn selection(&self, paths: &[String]) -> Result<Selection> {
        let mut out = Vec::new();
        for p in paths {
            let trimmed = p.trim_end_matches('/');
            if trimmed.is_empty() || trimmed == "/" {
                continue;
            }
            out.push(self.repo_path(trimmed)?);
        }
        Ok(Selection(out))
    }

    // ---- restore files ---------------------------------------------------------------------

    /// What restoring `paths` from `rev` changes: path and whether it is deleted.
    fn restore_hits(&self, cx: &Ctx, rev: ObjectId, paths: &[String]) -> Result<Vec<(String, bool)>> {
        let sel = self.selection(paths)?;
        let at = flatten(&self.repo, self.commit_tree(rev)?)?;
        let now = crate::undo::snapshot_tree(&cx.whole)?;
        let now = flatten(&self.repo, now)?;
        let mut out = Vec::new();
        for (path, blob) in &at {
            if sel.matches(path.as_slice()) && now.get(path).is_none_or(|n| !crate::worktree::same(*n, *blob)) {
                out.push((path.to_str_lossy().into_owned(), false));
            }
        }
        for path in now.keys() {
            if sel.matches(path.as_slice()) && !at.contains_key(path) {
                out.push((path.to_str_lossy().into_owned(), true));
            }
        }
        out.sort();
        Ok(out)
    }

    pub(crate) fn plan_restore_files(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        rev: &str,
        paths: &[String],
    ) -> Result<GitPlan> {
        if paths.is_empty() {
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                "Choose the files to restore.",
            )));
        }
        let Some(at) = self.try_resolve(rev) else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no commit \"{rev}\"."),
            )));
        };
        let hits = self.restore_hits(cx, at, paths)?;
        if hits.is_empty() {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                "These files are the same already.",
            )));
        }
        let deleted = hits.iter().filter(|(_, d)| *d).count();
        let changed: Vec<_> = hits.iter().filter(|(path, _)| {
            cx.changes().is_ok_and(|c| c.iter().any(|f| &f.path == path))
        }).collect();
        p.steps.push(format!(
            "Makes {} the way {} {}.",
            plural_s(hits.len(), "file"),
            if hits.len() == 1 { "it is" } else { "they are" },
            self.named(at)
        ));
        if deleted > 0 {
            p.steps.push(format!(
                "{} did not exist there and {} deleted.",
                plural(deleted, "file", "files"),
                if deleted == 1 { "is" } else { "are" }
            ));
        }
        if !changed.is_empty() {
            p.warnings.push(format!(
                "{} uncommitted changes. They are replaced; the backup keeps them for Undo.",
                plural(changed.len(), "file has", "files have"),
            ));
            p.destructive = true;
        }
        p.steps
            .push("Your branch and its commits do not move.".into());
        p.files = hits.iter().map(|(f, _)| format!("/{f}")).collect();
        p.backup = true;
        p.stop(cx.unsaved());
        p.stop(cx.in_progress()?);
        Ok(p)
    }

    pub(crate) fn run_restore_files(
        &self,
        cx: &Ctx,
        rev: &str,
        paths: &[String],
    ) -> Result<Done> {
        let at = self.resolve_rev(rev)?;
        let hits = self.restore_hits(cx, at, paths)?;
        let now = crate::undo::snapshot_tree(&cx.whole)?;
        let flat = flatten(&self.repo, self.commit_tree(at)?)?;
        let mut editor = self.repo.edit_tree(now).ctx("cannot build the tree")?;
        for (path, deleted) in &hits {
            let key = BString::from(path.as_str());
            match flat.get(&key) {
                Some(blob) if !deleted => editor
                    .upsert(
                        key.as_bstr(),
                        crate::commit::entry_kind(blob.mode),
                        blob.id,
                    )
                    .ctx("cannot build the tree")?,
                _ => editor.remove(key.as_bstr()).ctx("cannot build the tree")?,
            };
        }
        let want = editor.write().ctx("cannot write the tree")?.detach();
        cx.whole.move_worktree(now, want, "restore files")?;
        Ok(Done::new(
            format!(
                "Restored {} from {}.",
                plural_s(hits.len(), "file"),
                short(&at)
            ),
            true,
        ))
    }

    // ---- restore branch --------------------------------------------------------------------

    pub(crate) fn plan_restore_branch(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        name: &str,
        target: &str,
    ) -> Result<GitPlan> {
        if let Err(b) = check_ref_name(name) {
            return Ok(p.blocked_by(b));
        }
        let Some(to) = self.try_resolve(target) else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no commit \"{target}\"."),
            )));
        };
        if cx.cur.as_deref() == Some(name) {
            return Ok(p.blocked_by(block(
                GitBlockReason::CurrentBranch,
                format!("{name} is the branch you are on. Use Reset to move it."),
            )));
        }
        let full = branch_full(name);
        let old = self.ref_commit(&full)?;
        if old == Some(to) {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                format!("{name} is there already."),
            )));
        }
        match old {
            None => p.steps.push(format!(
                "Creates the branch {name} again at {}.",
                self.named(to)
            )),
            Some(old) => {
                p.steps
                    .push(format!("Moves {name} to {}.", self.named(to)));
                let leaving = self.count_hidden(old, &[to])?;
                if leaving > 0 {
                    p.warnings.push(format!(
                        "{} {name}. The backup keeps {} for Undo.",
                        plural(leaving, "commit leaves", "commits leave"),
                        if leaving == 1 { "it" } else { "them" }
                    ));
                }
                p.backup = true;
            }
        }
        p.refs.push(GitRefMove {
            name: full,
            before: old.map(|id| id.to_string()),
            after: Some(to.to_string()),
        });
        Ok(p)
    }

    pub(crate) fn run_restore_branch(
        &self,
        run: &mut Run<'_>,
        name: &str,
        target: &str,
    ) -> Result<Done> {
        let to = self.resolve_rev(target)?;
        let full = branch_full(name);
        let old = self.ref_commit(&full)?;
        if let Some(old) = old {
            run.backup(self, name, Some(old))?;
        }
        self.put_ref(
            &full,
            Some(to),
            old,
            &format!("requesttailor: restore {name} to {target}"),
        )?;
        Ok(Done::new(
            format!("{name} points at {} again.", self.named(to)),
            false,
        ))
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! `GitAction::Commit` (GitTailor wave 1, strand F): a commit as an action, with a plan, a
//! backup ref of the branch and a journal entry, so it can be undone like amend. The commit
//! itself is [`Repo::commit`] (the files named, as they are on disk) or [`Repo::commit_staged`]
//! (what the index holds), so the commit scanner, the signer and the merge conclusion are the
//! same as for a direct commit.
//!
//! Undo moves the branch back to where it was (an unborn branch: gone again); the working files
//! keep their content and show as changes. A commit that concluded a merge is undone without
//! bringing the merge back (the plan says so).

use gix::bstr::{BString, ByteSlice};
use tk_git_types::GitFileState;
use tk_git_types::actions::{GitBlockReason, GitPlan, GitRefMove};
use tk_git_types::index::GitStagedCommit;

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural_s};
use crate::branch_actions::diff_paths;
use crate::repo::Repo;
use crate::util::{Result, fail, flatten};

impl Repo {
    /// The branch `HEAD` points to (also unborn), as a full and a short name.
    fn commit_branch(cx: &Ctx) -> Option<(String, String)> {
        let full = cx.head_ref.as_ref()?.as_bstr().to_str_lossy().into_owned();
        let short = full.strip_prefix("refs/heads/")?.to_owned();
        Some((full, short))
    }

    /// `paths` of the action as repository paths (leading `/`: repository-relative; without
    /// one: relative to the scope).
    fn commit_paths(&self, paths: &[String]) -> Result<Vec<String>> {
        paths
            .iter()
            .map(|p| match p.strip_prefix('/') {
                Some(rel) => Ok(crate::blob::repo_path(rel)?.to_str_lossy().into_owned()),
                None => Ok(self.scope.abs(p)?.to_str_lossy().into_owned()),
            })
            .collect()
    }

    /// The files the commit takes (repository-relative, leading `/`).
    fn commit_files(&self, cx: &Ctx, paths: &[String], staged: bool) -> Result<Vec<String>> {
        if staged {
            let index = cx.whole.load_index()?;
            let tree = cx.whole.tree_from_index(&index)?;
            return Ok(diff_paths(
                &flatten(&self.repo, cx.whole.head_tree()?)?,
                &flatten(&self.repo, tree)?,
            ));
        }
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: Vec<BString> = self
            .commit_paths(paths)?
            .into_iter()
            .map(BString::from)
            .collect();
        Ok(cx
            .changes()?
            .iter()
            .filter(|f| wanted.iter().any(|w| w.as_slice() == f.path.as_bytes()))
            .map(|f| format!("/{}", f.path))
            .collect())
    }

    pub(crate) fn plan_commit(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        message: &str,
        paths: &[String],
        staged: bool,
    ) -> Result<GitPlan> {
        let Some((full, branch)) = Self::commit_branch(cx) else {
            return Ok(p.blocked_by(block(
                GitBlockReason::Detached,
                "You are not on a branch, so a new commit would belong to none. Create a branch here or switch to one first.",
            )));
        };
        let message = message.trim();
        if message.is_empty() {
            return Ok(p.blocked_by(block(
                GitBlockReason::InvalidName,
                "Write a commit message first.",
            )));
        }
        if !staged {
            p.stop(cx.unsaved());
        }
        let merging = cx.underway.is_some();
        let conflicted: Vec<String> = cx
            .changes()?
            .iter()
            .filter(|f| f.state == GitFileState::Conflicted)
            .map(|f| format!("/{}", f.path))
            .collect();
        let files = self.commit_files(cx, paths, staged)?;
        let open: Vec<&String> = conflicted
            .iter()
            .filter(|c| staged || !files.contains(c))
            .collect();
        if !open.is_empty() {
            p.stop(Some(block(
                GitBlockReason::InProgress,
                format!(
                    "Resolve the conflicts in {} first, then commit.",
                    plural_s(open.len(), "file")
                ),
            )));
        }
        if files.is_empty() && !merging {
            p.undoable = false;
            return Ok(p.blocked_by(block(
                GitBlockReason::NothingToDo,
                if staged {
                    "Nothing is staged."
                } else if paths.is_empty() {
                    "Choose the files to commit."
                } else {
                    "The selected files have no changes to commit."
                },
            )));
        }
        let subject = message.lines().next().unwrap_or("");
        p.steps.push(if merging {
            format!(
                "Finishes the merge in progress on {branch} with the commit \"{subject}\"{}.",
                if files.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", plural_s(files.len(), "file"))
                }
            )
        } else {
            format!(
                "Commits {} on {branch} as \"{subject}\".",
                plural_s(files.len(), "file")
            )
        });
        p.steps.push(match cx.head {
            Some(_) => format!(
                "Your files do not change. Undo moves {branch} back and keeps the changes in your files."
            ),
            None => format!(
                "This is the first commit of {branch}. Undo removes it again and keeps your files."
            ),
        });
        if merging {
            p.warnings.push(
                "Undoing this commit later does not start the merge again: the merged files stay as changes."
                    .into(),
            );
        }
        p.files = files;
        p.refs.push(GitRefMove {
            name: full,
            before: cx.head.map(|h| h.to_string()),
            after: Some(String::new()),
        });
        p.backup = cx.head.is_some();
        Ok(p)
    }

    pub(crate) fn run_commit(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        message: &str,
        paths: &[String],
        staged: bool,
    ) -> Result<Done> {
        let (full, branch) =
            Self::commit_branch(cx).ok_or_else(|| fail("You are not on a branch."))?;
        let first = run.backups.len();
        if cx.head.is_some() {
            run.backup(self, &branch, cx.head)?;
        }
        let made = match self.make_commit(cx, run, message, paths, staged) {
            Ok(made) => made,
            Err(e) => {
                // Refused (the scanner, the signer, an empty message) before the branch moved:
                // the backup of this run points where the branch still is, so it goes again and
                // a refused commit leaves no trace.
                if self.ref_value(&full)? == cx.head {
                    for name in run.backups.drain(first..) {
                        self.put_ref(&name, None, cx.head, "backup not needed")?;
                    }
                }
                return Err(e);
            }
        };
        Ok(Done::new(
            format!("Committed \"{}\" on {branch}.", made.summary),
            false,
        ))
    }

    fn make_commit(
        &self,
        cx: &Ctx,
        run: &Run<'_>,
        message: &str,
        paths: &[String],
        staged: bool,
    ) -> Result<tk_git_types::GitCommit> {
        Ok(if staged {
            cx.whole.commit_staged(
                &GitStagedCommit {
                    message: message.to_owned(),
                },
                run.opts.identity,
            )?
        } else {
            cx.whole
                .commit(message, &self.commit_paths(paths)?, run.opts.identity)?
        })
    }
}

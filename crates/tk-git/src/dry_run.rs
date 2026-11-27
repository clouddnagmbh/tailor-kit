// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Dry runs of Git actions (GitTailor wave 2, strand C): [`Repo::dry_run`] runs an action in an
//! object store in memory (gix object memory) and says exactly what it would do: the refs with
//! the real ids of the new commits, the history as it would be, the files and the conflicts.
//! Nothing in `.git` and nothing in the working tree changes, not even a loose object.
//! [`Repo::accept_dry_run`] writes exactly those objects and moves the refs to the previewed ids
//! (backup refs and a journal entry like [`Repo::apply`], so [`Repo::undo`] takes it back), when
//! nothing changed since; a dry run that cannot be reproduced exactly (`exact == false`: a merge
//! with conflicts, a commit that concludes a merge) is run again with [`Repo::apply`] instead.
//!
//! Dry runs exist for the actions that make or replace commits or move or delete a branch:
//! merge, pull (merging what was fetched last; a dry run downloads nothing), cherry-pick,
//! revert, reset, amend, rewrite, delete-branch and commit. Every other action is answered with
//! its plan only and accepted by running it.

use std::collections::{BTreeMap, HashMap};

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::{Flags, Stage};
use gix::object::tree::EntryKind;
use tk_git_types::actions::{
    GitAction, GitActionResult, GitPlan, GitRefMove, GitResetMode, GitUndoEntry,
};
use tk_git_types::graph::{GitChangedFile, GitGraphPage, GitRef, GitRefKind};
use tk_git_types::{GitDiff, GitFileChange, GitFileState, GitPullResult, text_diff};

use crate::actions::{ActionOptions, Ctx, JournalLine, Restore, Run, action_tag, now, short};
use crate::branch_actions::branch_full;
use crate::commit::{entry_kind, index_mode, signature};
use crate::merge::{MergeKind, ThreeWay};
use crate::repo::Repo;
use crate::undo::{snapshot_commit, snapshot_tree, tree_with};
use crate::util::{Blob, Context, Flat, GitError, Result, blob_bytes, fail, flatten};
use crate::worktree::{blocked, blocked_error, stat_of};

/// Commits the preview's graph shows at most (the newest around the moved refs).
pub const DRY_RUN_GRAPH_LIMIT: usize = 50;
/// Commits [`DryRunOutcome::left_behind`] names at most.
const LEFT_BEHIND_LIMIT: usize = 50;

/// What accepting a dry run does to the index besides a working-tree move.
#[derive(Debug, Clone)]
enum IndexEffect {
    Keep,
    /// The index for this tree (stats kept where still true): amend with files, reset.
    Tree(ObjectId),
    /// Exactly these paths take these entries (`None`: removed), as `Repo::commit` does.
    Paths(Vec<(BString, Option<(ObjectId, EntryKind)>)>),
}

/// What accepting a dry run changes.
#[derive(Debug, Clone)]
struct Effect {
    /// Full ref name, value before, value after (`None`: deleted / not there).
    refs: Vec<(String, Option<ObjectId>, Option<ObjectId>)>,
    /// The working files move from the first tree (as they are) to the second.
    worktree: Option<(ObjectId, ObjectId)>,
    /// A backup ref of the working files before (actions that throw uncommitted work away).
    backup_worktree: bool,
    index: IndexEffect,
    /// The new `.git/config` (delete-branch drops the branch's section).
    config: Option<String>,
    /// How a merge-like step ends.
    outcome: Option<GitPullResult>,
    /// What accepting says it did.
    message: String,
}

/// The repository as the dry run found it: accepting is refused once any of it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    head_ref: Option<String>,
    head: Option<ObjectId>,
    refs: Vec<(String, Option<ObjectId>)>,
    worktree: ObjectId,
    index: ObjectId,
    conflicted: bool,
    underway: bool,
    config: String,
}

/// A dry run: what the action would do, with the objects it wrote kept in memory.
pub struct DryRunOutcome {
    /// The action's plan (with `blocked` when it cannot run now).
    pub plan: GitPlan,
    /// What it would do, plain words.
    pub summary: String,
    /// Ref moves with the real ids of the result (none for a blocked plan).
    pub refs: Vec<GitRefMove>,
    /// The history as it would be: the newest commits around the moved refs, refs as afterwards.
    pub graph: GitGraphPage,
    /// Commits no branch, tag or remote branch reaches afterwards (the backup ref keeps them).
    pub left_behind: Vec<String>,
    /// Files from now to the result: the working files when they move, else the files of the
    /// branch's new commit against its old one.
    pub files: Vec<GitChangedFile>,
    /// Files that would conflict (repository-relative, leading `/`).
    pub conflicts: Vec<String>,
    pub warnings: Vec<String>,
    /// Accepting writes exactly these objects and moves the refs to these ids; `false`: it runs
    /// the action again (conflicts, a commit that concludes a merge, an action without a dry run).
    pub exact: bool,
    /// Accepting changes the working files.
    pub reload: bool,
    seen: Seen,
    effect: Option<Effect>,
    /// Per path of [`DryRunOutcome::files`]: before and after.
    sides: BTreeMap<BString, (Option<Blob>, Option<Blob>)>,
    objects: Vec<(ObjectId, gix::objs::Kind, Vec<u8>)>,
}

impl std::fmt::Debug for DryRunOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DryRunOutcome")
            .field("summary", &self.summary)
            .field("refs", &self.refs)
            .field("files", &self.files.len())
            .field("conflicts", &self.conflicts)
            .field("exact", &self.exact)
            .field("objects", &self.objects.len())
            .finish_non_exhaustive()
    }
}

impl DryRunOutcome {
    /// The object ids the dry run wrote (in memory), sorted: accepting writes exactly these.
    pub fn object_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .objects
            .iter()
            .map(|(id, _, _)| id.to_string())
            .collect();
        ids.sort();
        ids
    }

    /// Bytes the dry run keeps in memory (for a product's limits).
    pub fn memory_bytes(&self) -> usize {
        self.objects.iter().map(|(_, _, d)| d.len()).sum()
    }
}

/// What a simulated action leaves.
struct Sim {
    effect: Effect,
    /// The tree the files list starts from and the one it ends at.
    from: ObjectId,
    to: ObjectId,
    conflicts: Vec<String>,
    exact: bool,
    warnings: Vec<String>,
}

/// Whether bytes are binary for display (as the graph's diff tells).
fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8000)].contains(&0) || std::str::from_utf8(bytes).is_err()
}

fn oid(id: &Option<ObjectId>) -> Option<String> {
    id.map(|i| i.to_string())
}

impl Repo {
    /// The whole repository with an object store in memory: everything written through it stays
    /// in this instance.
    fn in_memory(&self) -> Result<Repo> {
        let mut whole = self.whole()?;
        whole.repo = whole.repo.with_object_memory();
        Ok(whole)
    }

    /// The repository as it is now, read through `m` (objects it computes stay in memory).
    fn seen(m: &Repo, names: &[String]) -> Result<Seen> {
        let index = m.load_index()?;
        let conflicted = index
            .entries()
            .iter()
            .any(|e| e.stage() != Stage::Unconflicted);
        let mut refs = Vec::new();
        for n in names {
            refs.push((n.clone(), m.ref_value(n)?));
        }
        Ok(Seen {
            head_ref: m.head_ref()?.map(|n| n.as_bstr().to_string()),
            head: m.head_commit()?,
            refs,
            worktree: snapshot_tree(m)?,
            index: m.tree_from_index(&index)?,
            conflicted,
            underway: m.underway()?.is_some(),
            config: m.config_text()?,
        })
    }

    /// Runs `action` in memory (see the module docs). Reads only: `.git` and the working tree
    /// stay byte for byte as they are.
    pub fn dry_run(&self, action: &GitAction, opts: &ActionOptions<'_>) -> Result<DryRunOutcome> {
        let cx = self.ctx_with(self.in_memory()?, opts.unsaved)?;
        let m = &cx.whole;
        let plan = m.plan_with(&cx, action)?;
        let sim = if plan.blocked.is_some() {
            None
        } else {
            m.simulate(&cx, action, &plan, opts)?
        };
        let mut names: Vec<String> = vec!["HEAD".into()];
        if let Some(h) = &cx.head_ref {
            names.push(h.as_bstr().to_string());
        }
        for r in &plan.refs {
            if !names.contains(&r.name) {
                names.push(r.name.clone());
            }
        }
        if let Some(s) = &sim {
            for (n, _, _) in &s.effect.refs {
                if !names.contains(n) {
                    names.push(n.clone());
                }
            }
        }
        let seen = Self::seen(m, &names)?;
        let moves: Vec<(String, Option<ObjectId>, Option<ObjectId>)> = sim
            .as_ref()
            .map(|s| s.effect.refs.clone())
            .unwrap_or_default();
        let (graph, left_behind) = m.graph_after(&cx, &moves)?;
        let (sides, files) = match &sim {
            Some(s) => m.dry_sides(s.from, s.to)?,
            None => (BTreeMap::new(), Vec::new()),
        };
        let summary = match &sim {
            Some(s) => s.effect.message.clone(),
            None => plan
                .blocked
                .as_ref()
                .map(|b| b.message.clone())
                .unwrap_or_default(),
        };
        let mut warnings = plan.warnings.clone();
        let (effect, conflicts, exact, reload) = match sim {
            Some(s) => {
                warnings.extend(s.warnings);
                let reload = s.effect.worktree.is_some() || !s.exact;
                let exact = s.exact;
                (Some(s.effect), s.conflicts, exact, reload)
            }
            None => (None, Vec::new(), false, false),
        };
        let refs = moves
            .iter()
            .map(|(n, b, a)| GitRefMove {
                name: n.clone(),
                before: oid(b),
                after: oid(a),
            })
            .collect();
        let objects = match m.repo.objects.reset_object_memory() {
            Some(mut storage) => std::mem::take(&mut *storage)
                .into_iter()
                .map(|(id, (kind, data))| (id, kind, data))
                .collect(),
            None => Vec::new(),
        };
        Ok(DryRunOutcome {
            plan,
            summary,
            refs,
            graph,
            left_behind,
            files,
            conflicts,
            warnings,
            exact,
            reload,
            seen,
            effect,
            sides,
            objects,
        })
    }

    /// The simulation of `action` on `self` (the in-memory whole repository of `cx`); `None`
    /// for actions without a dry run of their own.
    fn simulate(
        &self,
        cx: &Ctx,
        action: &GitAction,
        plan: &GitPlan,
        opts: &ActionOptions<'_>,
    ) -> Result<Option<Sim>> {
        Ok(Some(match action {
            GitAction::Merge { from } => {
                let theirs = self.resolve_rev(from)?;
                let ff = plan.merge.as_ref().is_some_and(|m| m.fast_forward);
                self.sim_merge(cx, theirs, from, ff, opts, "merge")?
            }
            GitAction::Pull => {
                let up = self
                    .upstream()?
                    .ok_or_else(|| fail("This branch has no upstream yet. Publish it first."))?;
                let Some(theirs) = self.ref_commit(&up.tracking)? else {
                    return Err(fail(format!(
                        "Nothing was fetched from {} yet; a dry run does not download anything.",
                        up.display
                    )));
                };
                let head = cx
                    .head
                    .ok_or_else(|| fail("Make the first commit first."))?;
                if self.is_ancestor(theirs, head) {
                    return Err(fail(format!(
                        "{} has nothing new since the last fetch; a dry run does not download anything.",
                        up.display
                    )));
                }
                let ff = self.is_ancestor(head, theirs);
                let mut sim = self.sim_merge(cx, theirs, &up.display, ff, opts, "pull")?;
                sim.warnings.push(format!(
                    "The dry run merges {} as it was fetched last ({}). Accepting does not download anything; pull again afterwards for newer commits.",
                    up.display,
                    short(&theirs)
                ));
                sim
            }
            GitAction::CherryPick { commit } => self.sim_pick(cx, commit, true, opts)?,
            GitAction::Revert { commit } => self.sim_pick(cx, commit, false, opts)?,
            GitAction::Reset { target, mode } => self.sim_reset(cx, target, *mode)?,
            GitAction::Amend {
                message,
                paths,
                staged,
            } => self.sim_amend(cx, message.as_deref(), paths, *staged, opts)?,
            GitAction::Rewrite {
                branch,
                tip,
                summary,
            } => self.sim_rewrite(cx, branch, tip, summary)?,
            GitAction::DeleteBranch { name, .. } => self.sim_delete_branch(name)?,
            GitAction::Commit {
                message,
                paths,
                staged,
            } => self.sim_commit(cx, message, paths, *staged, opts)?,
            _ => return Ok(None),
        }))
    }

    fn on_branch(cx: &Ctx) -> Result<(ObjectId, String)> {
        match (cx.head, cx.cur.clone()) {
            (Some(h), Some(c)) => Ok((h, c)),
            _ => Err(fail("You are not on a branch.")),
        }
    }

    /// A merge of `theirs` into the current branch: fast-forward, a merge commit, or conflicts.
    fn sim_merge(
        &self,
        cx: &Ctx,
        theirs: ObjectId,
        label: &str,
        ff: bool,
        opts: &ActionOptions<'_>,
        what: &str,
    ) -> Result<Sim> {
        let (head, cur) = Self::on_branch(cx)?;
        let head_tree = self.commit_tree(head)?;
        let full = branch_full(&cur);
        if ff {
            let commits = u32::try_from(self.count_hidden(theirs, &[head])?).unwrap_or(u32::MAX);
            let to = self.commit_tree(theirs)?;
            return Ok(Sim {
                effect: Effect {
                    refs: vec![(full, Some(head), Some(theirs))],
                    worktree: Some((head_tree, to)),
                    backup_worktree: false,
                    index: IndexEffect::Keep,
                    config: None,
                    outcome: Some(GitPullResult::FastForward { commits }),
                    message: format!("Moved {cur} forward to {label}."),
                },
                from: head_tree,
                to,
                conflicts: Vec::new(),
                exact: true,
                warnings: Vec::new(),
            });
        }
        let tw = ThreeWay {
            kind: MergeKind::Merge,
            base: None,
            ours: head,
            theirs,
            label,
        };
        let message = format!("Merge {label} into {cur}");
        self.sim_three_way(cx, &tw, None, &message, opts, what)
    }

    /// A three-way merge onto `HEAD` and its commit (one parent unless it is a merge).
    fn sim_three_way(
        &self,
        cx: &Ctx,
        tw: &ThreeWay<'_>,
        author: Option<gix::actor::Signature>,
        message: &str,
        opts: &ActionOptions<'_>,
        what: &str,
    ) -> Result<Sim> {
        let (head, cur) = Self::on_branch(cx)?;
        let head_tree = self.commit_tree(head)?;
        let computed = self.compute_merge(&self.repo, tw)?;
        let (tree, texts, other) = self.pick_outcome(computed)?;
        let mut conflicts: Vec<String> = texts.into_iter().map(|c| c.path).chain(other).collect();
        conflicts.sort();
        conflicts.dedup();
        let full = branch_full(&cur);
        if !conflicts.is_empty() {
            // Accepting runs the action again: it stops on the conflicts like `git merge`.
            return Ok(Sim {
                effect: Effect {
                    refs: Vec::new(),
                    worktree: None,
                    backup_worktree: false,
                    index: IndexEffect::Keep,
                    config: None,
                    outcome: Some(GitPullResult::Conflicts {
                        paths: conflicts.clone(),
                    }),
                    message: format!(
                        "{what} would stop on conflicts in {} file{}.",
                        conflicts.len(),
                        if conflicts.len() == 1 { "" } else { "s" }
                    ),
                },
                from: head_tree,
                to: tree,
                conflicts,
                exact: false,
                warnings: Vec::new(),
            });
        }
        let identity = self.identity_or(opts.identity)?;
        let committer = signature(&identity);
        let author = author.unwrap_or_else(|| committer.clone());
        let parents: Vec<ObjectId> = match tw.kind {
            MergeKind::Merge => vec![head, tw.theirs],
            MergeKind::CherryPick | MergeKind::Revert => vec![head],
        };
        let new = self.write_commit_object(gix::objs::Commit {
            tree,
            parents: parents.into(),
            author,
            committer,
            encoding: None,
            message: message.into(),
            extra_headers: Vec::new(),
        })?;
        let text = match tw.kind {
            MergeKind::Merge => format!("Merged {} into {cur}.", tw.label),
            MergeKind::CherryPick => format!("Copied \"{}\" onto {cur}.", self.subject(tw.theirs)),
            MergeKind::Revert => format!(
                "Undid \"{}\" with a new commit.",
                self.subject(tw.base.unwrap_or(tw.theirs))
            ),
        };
        Ok(Sim {
            effect: Effect {
                refs: vec![(full, Some(head), Some(new))],
                worktree: Some((head_tree, tree)),
                backup_worktree: false,
                index: IndexEffect::Keep,
                config: None,
                outcome: Some(GitPullResult::Merged {
                    oid: new.to_string(),
                }),
                message: text,
            },
            from: head_tree,
            to: tree,
            conflicts: Vec::new(),
            exact: true,
            warnings: Vec::new(),
        })
    }

    fn sim_pick(
        &self,
        cx: &Ctx,
        commit: &str,
        pick: bool,
        opts: &ActionOptions<'_>,
    ) -> Result<Sim> {
        let (head, _) = Self::on_branch(cx)?;
        let id = self.resolve_rev(commit)?;
        let picked = self.repo.find_commit(id).ctx("cannot read a commit")?;
        let parents: Vec<ObjectId> = picked.parent_ids().map(|p| p.detach()).collect();
        let label = id.to_hex_with_len(7).to_string();
        if pick {
            let decoded = picked.decode().ctx("cannot read a commit")?;
            let author: gix::actor::Signature = decoded
                .author()
                .ctx("cannot read a commit")?
                .to_owned()
                .ctx("cannot read a commit")?;
            let message = decoded.message.to_str_lossy().into_owned();
            let tw = ThreeWay {
                kind: MergeKind::CherryPick,
                base: parents.first().copied(),
                ours: head,
                theirs: id,
                label: &label,
            };
            self.sim_three_way(cx, &tw, Some(author), &message, opts, "Copying the commit")
        } else {
            let parent = *parents
                .first()
                .ok_or_else(|| fail("This is the very first commit."))?;
            let tw = crate::revert::three_way(head, id, parent, &label);
            let message = crate::revert::message(&self.subject(id), id);
            self.sim_three_way(cx, &tw, None, &message, opts, "Undoing the commit")
        }
    }

    /// `to`'s tree with the untracked files of `now` that it does not have (what reset and
    /// rewrite leave on disk).
    fn keep_untracked(&self, cx: &Ctx, now: ObjectId, to_tree: ObjectId) -> Result<ObjectId> {
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
                    .upsert(path.as_bstr(), entry_kind(blob.mode), blob.id)
                    .ctx("cannot build the tree")?;
            }
        }
        Ok(editor.write().ctx("cannot write the tree")?.detach())
    }

    fn sim_reset(&self, cx: &Ctx, target: &str, mode: GitResetMode) -> Result<Sim> {
        let (head, cur) = Self::on_branch(cx)?;
        let to = self.resolve_rev(target)?;
        let to_tree = self.commit_tree(to)?;
        let head_tree = self.commit_tree(head)?;
        let refs = vec![(branch_full(&cur), Some(head), Some(to))];
        let message = format!("Moved {cur} to {}.", short(&to));
        Ok(match mode {
            GitResetMode::KeepChanges => Sim {
                effect: Effect {
                    refs,
                    worktree: None,
                    backup_worktree: false,
                    index: IndexEffect::Tree(to_tree),
                    config: None,
                    outcome: None,
                    message,
                },
                from: head_tree,
                to: to_tree,
                conflicts: Vec::new(),
                exact: true,
                warnings: Vec::new(),
            },
            GitResetMode::Discard => {
                let now = snapshot_tree(self)?;
                let want = self.keep_untracked(cx, now, to_tree)?;
                Sim {
                    effect: Effect {
                        refs,
                        worktree: Some((now, want)),
                        backup_worktree: true,
                        index: IndexEffect::Keep,
                        config: None,
                        outcome: None,
                        message,
                    },
                    from: now,
                    to: want,
                    conflicts: Vec::new(),
                    exact: true,
                    warnings: Vec::new(),
                }
            }
        })
    }

    fn sim_amend(
        &self,
        cx: &Ctx,
        message: Option<&str>,
        paths: &[String],
        staged: bool,
        opts: &ActionOptions<'_>,
    ) -> Result<Sim> {
        let (head, cur) = Self::on_branch(cx)?;
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
        let head_tree = self.head_tree()?;
        // The tree as `amend.rs` builds it: the index, or HEAD's with `paths` from disk.
        let content = if staged {
            let index = self.load_index()?;
            let tree = self.tree_from_index(&index)?;
            (tree != head_tree).then_some(tree)
        } else if paths.is_empty() {
            None
        } else {
            let sel = self.selection(paths)?;
            let files: Vec<_> = cx
                .changes()?
                .iter()
                .filter(|f| f.state != GitFileState::Conflicted && sel.matches(f.path.as_bytes()))
                .collect();
            if files.is_empty() {
                None
            } else {
                Some(tree_with(self, head_tree, &files)?)
            }
        };
        let tree = content.unwrap_or(head_tree);
        let message = match message.map(str::trim) {
            Some(m) if !m.is_empty() => format!("{m}\n"),
            _ => old_message,
        };
        let identity = self.identity_or(opts.identity)?;
        let new = self.write_commit_object(gix::objs::Commit {
            tree,
            parents: parents.into(),
            author,
            committer: signature(&identity),
            encoding,
            message: message.as_str().into(),
            extra_headers: Vec::new(),
        })?;
        Ok(Sim {
            effect: Effect {
                refs: vec![(branch_full(&cur), Some(head), Some(new))],
                worktree: None,
                backup_worktree: false,
                index: match content {
                    Some(t) => IndexEffect::Tree(t),
                    None => IndexEffect::Keep,
                },
                config: None,
                outcome: None,
                message: format!(
                    "Replaced the last commit on {cur} with \"{}\".",
                    message.lines().next().unwrap_or("")
                ),
            },
            from: head_tree,
            to: tree,
            conflicts: Vec::new(),
            exact: true,
            warnings: Vec::new(),
        })
    }

    fn sim_rewrite(&self, cx: &Ctx, branch: &str, tip: &str, summary: &str) -> Result<Sim> {
        let full = branch_full(branch);
        let old = self
            .ref_commit(&full)?
            .ok_or_else(|| fail(format!("There is no branch \"{branch}\".")))?;
        let new = self.resolve_rev(tip)?;
        let message = if summary.trim().is_empty() {
            format!("Rewrote {branch}.")
        } else {
            format!("{}.", summary.trim().trim_end_matches('.'))
        };
        let (old_tree, new_tree) = (self.commit_tree(old)?, self.commit_tree(new)?);
        let refs = vec![(full, Some(old), Some(new))];
        if cx.cur.as_deref() != Some(branch) {
            return Ok(Sim {
                effect: Effect {
                    refs,
                    worktree: None,
                    backup_worktree: false,
                    index: IndexEffect::Keep,
                    config: None,
                    outcome: None,
                    message,
                },
                from: old_tree,
                to: new_tree,
                conflicts: Vec::new(),
                exact: true,
                warnings: Vec::new(),
            });
        }
        let now = snapshot_tree(self)?;
        let want = self.keep_untracked(cx, now, new_tree)?;
        Ok(Sim {
            effect: Effect {
                refs,
                worktree: Some((now, want)),
                backup_worktree: false,
                index: IndexEffect::Keep,
                config: None,
                outcome: None,
                message,
            },
            from: now,
            to: want,
            conflicts: Vec::new(),
            exact: true,
            warnings: Vec::new(),
        })
    }

    /// `.git/config` without `[branch "name"]`, as `remove_config_section` would write it.
    fn config_without_branch(&self, name: &str) -> Result<String> {
        let path = self.repo.common_dir().join("config");
        let mut file = gix::config::File::from_path_no_includes(path, gix::config::Source::Local)
            .ctx("cannot read the repository configuration")?;
        let mut removed = false;
        while file.remove_section("branch", Some(name.into())).is_some() {
            removed = true;
        }
        if !removed {
            return self.config_text();
        }
        let mut out = Vec::new();
        file.write_to(&mut out)
            .ctx("cannot write the repository configuration")?;
        String::from_utf8(out).map_err(|_| fail("the configuration is not UTF-8"))
    }

    fn sim_delete_branch(&self, name: &str) -> Result<Sim> {
        let full = branch_full(name);
        let at = self
            .ref_value(&full)?
            .ok_or_else(|| fail(format!("There is no branch {name}.")))?;
        let config = self.config_without_branch(name)?;
        let config = (config != self.config_text()?).then_some(config);
        let tree = self.commit_tree(at)?;
        Ok(Sim {
            effect: Effect {
                refs: vec![(full, Some(at), None)],
                worktree: None,
                backup_worktree: false,
                index: IndexEffect::Keep,
                config,
                outcome: None,
                message: format!("Deleted {name}."),
            },
            from: tree,
            to: tree,
            conflicts: Vec::new(),
            exact: true,
            warnings: Vec::new(),
        })
    }

    fn sim_commit(
        &self,
        cx: &Ctx,
        message: &str,
        paths: &[String],
        staged: bool,
        opts: &ActionOptions<'_>,
    ) -> Result<Sim> {
        let Some(head_ref) = cx.head_ref.as_ref() else {
            return Err(fail("You are not on a branch."));
        };
        let full = head_ref.as_bstr().to_string();
        let branch = full.strip_prefix("refs/heads/").unwrap_or(&full).to_owned();
        let message = message.trim();
        let head_tree = self.head_tree()?;
        let parents: Vec<ObjectId> = cx.head.into_iter().collect();
        let index = self.load_index()?;
        let (tree, effect_index, files) = if staged {
            let tree = self.tree_from_index(&index)?;
            let head = flatten(&self.repo, head_tree)?;
            let mut files = Vec::new();
            for entry in index.entries() {
                if entry.stage() != Stage::Unconflicted
                    || entry.mode == gix::index::entry::Mode::COMMIT
                {
                    continue;
                }
                let path = entry.path(&index);
                if head.get(path).is_some_and(|b| b.id == entry.id) {
                    continue;
                }
                files.push((self.api_path(path), blob_bytes(&self.repo, entry.id)?));
            }
            (tree, IndexEffect::Keep, files)
        } else {
            let mut editor = self
                .repo
                .edit_tree(head_tree)
                .ctx("cannot read the HEAD tree")?;
            let mut pipeline = crate::filter::Filters::new(self)?;
            let mut updates = Vec::new();
            let mut files = Vec::new();
            for p in paths {
                let repo_path = match p.strip_prefix('/') {
                    Some(rel) => self.repo_path(rel)?,
                    None => self.scope.abs(p)?,
                };
                match pipeline.worktree_file_to_object(repo_path.as_bstr(), &index)? {
                    Some((_, EntryKind::Commit, _)) => {
                        return Err(fail(format!("{p} is a nested repository")));
                    }
                    Some((id, kind, _)) => {
                        editor
                            .upsert(repo_path.as_bstr(), kind, id)
                            .ctx("cannot build the tree")?;
                        files.push((
                            repo_path.to_str_lossy().into_owned(),
                            blob_bytes(&self.repo, id)?,
                        ));
                        updates.push((repo_path, Some((id, kind))));
                    }
                    None => {
                        editor
                            .remove(repo_path.as_bstr())
                            .ctx("cannot build the tree")?;
                        updates.push((repo_path, None));
                    }
                }
            }
            let tree = editor.write().ctx("cannot write the tree")?.detach();
            (tree, IndexEffect::Paths(updates), files)
        };
        let view: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(p, b)| (p.as_str(), b.as_slice()))
            .collect();
        self.scan_commit(&view)?;
        if cx.underway.is_some() {
            // Concluding a merge: accepting runs the commit itself (merge state and all).
            return Ok(Sim {
                effect: Effect {
                    refs: Vec::new(),
                    worktree: None,
                    backup_worktree: false,
                    index: IndexEffect::Keep,
                    config: None,
                    outcome: None,
                    message: format!("Would finish the merge with \"{message}\"."),
                },
                from: head_tree,
                to: tree,
                conflicts: Vec::new(),
                exact: false,
                warnings: Vec::new(),
            });
        }
        let identity = self.identity_or(opts.identity)?;
        let sig = signature(&identity);
        let new = self.write_commit_object(gix::objs::Commit {
            tree,
            parents: parents.into(),
            author: sig.clone(),
            committer: sig,
            encoding: None,
            message: message.into(),
            extra_headers: Vec::new(),
        })?;
        Ok(Sim {
            effect: Effect {
                refs: vec![(full, cx.head, Some(new))],
                worktree: None,
                backup_worktree: false,
                index: effect_index,
                config: None,
                outcome: None,
                message: format!(
                    "Committed \"{}\" on {branch}.",
                    message.lines().next().unwrap_or("")
                ),
            },
            from: head_tree,
            to: tree,
            conflicts: Vec::new(),
            exact: true,
            warnings: Vec::new(),
        })
    }

    /// Before and after of every path that differs between the trees `from` and `to`, and
    /// the list of changed files.
    #[allow(clippy::type_complexity)]
    fn dry_sides(
        &self,
        from: ObjectId,
        to: ObjectId,
    ) -> Result<(
        BTreeMap<BString, (Option<Blob>, Option<Blob>)>,
        Vec<GitChangedFile>,
    )> {
        let (a, b) = (flatten(&self.repo, from)?, flatten(&self.repo, to)?);
        let mut sides = BTreeMap::new();
        for p in a.keys().chain(b.keys()) {
            let (x, y) = (a.get(p).copied(), b.get(p).copied());
            if x != y {
                sides.insert(p.clone(), (x, y));
            }
        }
        let mut files = Vec::new();
        for (p, (x, y)) in &sides {
            let change = match (x, y) {
                (None, Some(_)) => GitFileChange::Added,
                (Some(_), None) => GitFileChange::Deleted,
                _ => GitFileChange::Modified,
            };
            let mut binary = false;
            for blob in [x, y].into_iter().flatten() {
                binary |= blob.mode == gix::index::entry::Mode::COMMIT
                    || is_binary(&blob_bytes(&self.repo, blob.id)?);
            }
            files.push(GitChangedFile {
                path: format!("/{}", p.to_str_lossy()),
                change,
                from: None,
                binary,
                extra: tk_git_types::NoExtra {},
            });
        }
        Ok((sides, files))
    }

    /// The graph as it would be after `moves`, and the commits it leaves behind.
    fn graph_after(
        &self,
        cx: &Ctx,
        moves: &[(String, Option<ObjectId>, Option<ObjectId>)],
    ) -> Result<(GitGraphPage, Vec<String>)> {
        let mut refs = self.refs()?;
        let current = cx.head_ref.as_ref().map(|h| h.as_bstr().to_string());
        for (name, _, after) in moves {
            match (refs.iter().position(|r| &r.full == name), after) {
                (Some(i), None) => {
                    refs.remove(i);
                }
                (Some(i), Some(id)) => refs[i].target = id.to_string(),
                (None, Some(id)) => {
                    let (kind, short_name) = if let Some(n) = name.strip_prefix("refs/heads/") {
                        (GitRefKind::Local, n.to_owned())
                    } else if let Some(n) = name.strip_prefix("refs/tags/") {
                        (GitRefKind::Tag, n.to_owned())
                    } else {
                        continue;
                    };
                    refs.push(GitRef {
                        full: name.clone(),
                        name: short_name,
                        kind,
                        target: id.to_string(),
                        current: current.as_deref() == Some(name.as_str()),
                        upstream: None,
                        ahead: 0,
                        behind: 0,
                        message: None,
                    });
                }
                (None, None) => {}
            }
        }
        // Ahead and behind of the moved local branches against their upstream.
        let targets: HashMap<String, String> = refs
            .iter()
            .map(|r| (r.full.clone(), r.target.clone()))
            .collect();
        for r in refs.iter_mut() {
            if r.kind != GitRefKind::Local || !moves.iter().any(|(n, _, _)| n == &r.full) {
                continue;
            }
            let Some(up) = &r.upstream else { continue };
            let Some(up_target) = targets.get(&format!("refs/remotes/{up}")) else {
                continue;
            };
            let (mine, theirs) = (
                ObjectId::from_hex(r.target.as_bytes()).ctx("invalid id")?,
                ObjectId::from_hex(up_target.as_bytes()).ctx("invalid id")?,
            );
            r.ahead = u32::try_from(self.count_hidden(mine, &[theirs])?).unwrap_or(u32::MAX);
            r.behind = u32::try_from(self.count_hidden(theirs, &[mine])?).unwrap_or(u32::MAX);
        }
        let head = match &current {
            Some(full) => match moves.iter().find(|(n, _, _)| n == full) {
                Some((_, _, after)) => *after,
                None => cx.head,
            },
            None => cx.head,
        };
        let mut tips: Vec<ObjectId> = moves.iter().filter_map(|(_, _, a)| *a).collect();
        tips.extend(head);
        tips.sort();
        tips.dedup();
        let order = self.recent_order(&tips, DRY_RUN_GRAPH_LIMIT)?;
        let mut labels = Self::ref_labels(&refs);
        let mut commits = Vec::with_capacity(order.len());
        for (id, parents) in &order {
            commits.push(self.graph_commit(*id, parents, &mut labels)?);
        }
        // Commits only the old values reach.
        let befores: Vec<ObjectId> = moves
            .iter()
            .filter_map(|(_, b, a)| b.filter(|b| Some(*b) != *a))
            .collect();
        let mut left_behind = Vec::new();
        if !befores.is_empty() {
            let mut hidden: Vec<ObjectId> = refs
                .iter()
                .filter_map(|r| ObjectId::from_hex(r.target.as_bytes()).ok())
                .collect();
            hidden.extend(head);
            let walk = self
                .repo
                .rev_walk(befores)
                .with_hidden(hidden)
                .all()
                .ctx("cannot walk the history")?;
            for info in walk.take(LEFT_BEHIND_LIMIT) {
                left_behind.push(info.ctx("cannot walk the history")?.id.to_string());
            }
        }
        Ok((
            GitGraphPage {
                more: commits.len() >= DRY_RUN_GRAPH_LIMIT,
                commits,
                refs,
                head: head.map(|h| h.to_string()),
                detached: head.is_some() && current.is_none(),
            },
            left_behind,
        ))
    }

    /// The newest `limit` commits reachable from `tips`, children before parents.
    fn recent_order(
        &self,
        tips: &[ObjectId],
        limit: usize,
    ) -> Result<Vec<(ObjectId, Vec<ObjectId>)>> {
        if tips.is_empty() {
            return Ok(Vec::new());
        }
        let walk = self
            .repo
            .rev_walk(tips.iter().copied())
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                Default::default(),
            ))
            .all()
            .ctx("cannot walk the history")?;
        let mut nodes: Vec<(ObjectId, i64, Vec<ObjectId>)> = Vec::new();
        for info in walk.take(limit) {
            let info = info.ctx("cannot walk the history")?;
            nodes.push((
                info.id,
                info.commit_time.unwrap_or_default(),
                info.parent_ids.iter().copied().collect(),
            ));
        }
        let index: HashMap<ObjectId, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.0, i)).collect();
        let mut children = vec![0u32; nodes.len()];
        for n in &nodes {
            for p in &n.2 {
                if let Some(&i) = index.get(p) {
                    children[i] += 1;
                }
            }
        }
        let mut ready: std::collections::BinaryHeap<(i64, std::cmp::Reverse<usize>)> = nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| children[*i] == 0)
            .map(|(i, n)| (n.1, std::cmp::Reverse(i)))
            .collect();
        let mut out = Vec::with_capacity(nodes.len());
        while let Some((_, std::cmp::Reverse(i))) = ready.pop() {
            for p in nodes[i].2.clone() {
                if let Some(&j) = index.get(&p) {
                    children[j] -= 1;
                    if children[j] == 0 {
                        ready.push((nodes[j].1, std::cmp::Reverse(j)));
                    }
                }
            }
            out.push((nodes[i].0, std::mem::take(&mut nodes[i].2)));
        }
        Ok(out)
    }

    /// The bytes of a blob of a dry run: from its memory, else from the repository.
    fn dry_blob(&self, outcome: &DryRunOutcome, id: ObjectId) -> Result<Vec<u8>> {
        if let Some((_, _, data)) = outcome.objects.iter().find(|(o, _, _)| *o == id) {
            return Ok(data.clone());
        }
        blob_bytes(&self.repo, id)
    }

    /// The line diff of one file of [`DryRunOutcome::files`] (`path` with a leading `/`).
    pub fn dry_run_file_diff(&self, outcome: &DryRunOutcome, path: &str) -> Result<GitDiff> {
        let rel = BString::from(path.strip_prefix('/').unwrap_or(path));
        let Some((before, after)) = outcome.sides.get(&rel) else {
            return Err(fail(format!("The dry run does not change {path}.")));
        };
        let text = |b: &Option<Blob>| -> Result<Option<Vec<u8>>> {
            match b {
                Some(blob) if blob.mode != gix::index::entry::Mode::COMMIT => {
                    Ok(Some(self.dry_blob(outcome, blob.id)?))
                }
                _ => Ok(None),
            }
        };
        let (old, new) = (text(before)?, text(after)?);
        let shown = format!("/{}", rel.to_str_lossy());
        let mut diff = text_diff(&shown, old.as_deref(), new.as_deref());
        if !diff.binary
            && (old.as_deref().is_some_and(is_binary) || new.as_deref().is_some_and(is_binary))
        {
            diff.binary = true;
            diff.hunks.clear();
        }
        Ok(diff)
    }

    /// Why accepting `outcome` would be refused now (the repository changed since the dry run),
    /// in plain words; `None`: it can be accepted. Reads only.
    pub fn dry_run_stale(&self, outcome: &DryRunOutcome) -> Result<Option<String>> {
        let m = self.in_memory()?;
        let names: Vec<String> = outcome.seen.refs.iter().map(|(n, _)| n.clone()).collect();
        let now = Self::seen(&m, &names)?;
        let was = &outcome.seen;
        let mut what = Vec::new();
        if now.head_ref != was.head_ref || now.head != was.head || now.refs != was.refs {
            what.push("the branches");
        }
        if now.worktree != was.worktree {
            what.push("the working files");
        }
        if now.index != was.index || now.conflicted != was.conflicted {
            what.push("the staged changes");
        }
        if now.underway != was.underway {
            what.push("the merge in progress");
        }
        if now.config != was.config {
            what.push("the repository settings");
        }
        Ok((!what.is_empty()).then(|| {
            format!(
                "{} changed since the dry run, so it no longer shows what would happen. Try it again.",
                capitalized(&what.join(" and "))
            )
        }))
    }

    /// Accepts a dry run (see the module docs): refused when the repository changed since
    /// ([`Repo::dry_run_stale`]) or the plan is blocked; else the objects are written, backups
    /// and a journal entry made and the refs moved to the previewed ids.
    pub fn accept_dry_run(
        &self,
        outcome: &DryRunOutcome,
        opts: &ActionOptions<'_>,
    ) -> Result<GitActionResult> {
        if let Some(b) = &outcome.plan.blocked {
            return Err(GitError::Failed(b.message.clone()));
        }
        if let Some(stale) = self.dry_run_stale(outcome)? {
            return Err(fail(stale));
        }
        let Some(effect) = outcome.effect.as_ref().filter(|_| outcome.exact) else {
            // Run again; a pull merges what was fetched, as its dry run did (no download).
            let action = match (&outcome.plan.action, self.upstream()?) {
                (GitAction::Pull, Some(up)) => GitAction::Merge { from: up.display },
                (a, _) => a.clone(),
            };
            return self.apply(&action, opts);
        };
        if opts.unsaved && (effect.worktree.is_some() || !matches!(effect.index, IndexEffect::Keep))
        {
            return Err(fail("Save your changes in the app first."));
        }
        let whole = self.whole()?;
        // The working files first: nothing moves when a file in the way was edited (checked
        // against the state the dry run saw, which is still the state now).
        if let Some((from, to)) = effect.worktree {
            let (from_flat, to_flat) = (self.dry_flat(outcome, from)?, self.dry_flat(outcome, to)?);
            let index = whole.load_index()?;
            let stuck = blocked(&whole, &from_flat, &to_flat, &index)?;
            if !stuck.is_empty() {
                return Err(blocked_error(&whole, "accept the dry run", &stuck));
            }
        }
        // Exactly the previewed objects.
        for (id, kind, data) in &outcome.objects {
            if self.repo.has_object(id) {
                continue;
            }
            let written = gix::objs::Write::write_buf(&self.repo, *kind, data)
                .ctx("cannot write an object of the dry run")?;
            if written != *id {
                return Err(fail("an object of the dry run came out different"));
            }
        }
        let action = &outcome.plan.action;
        let time = now();
        let mut run = Run {
            opts,
            time,
            backups: Vec::new(),
            pushed: None,
        };
        let mut names: Vec<String> = Vec::new();
        for (n, _, _) in &effect.refs {
            names.push(n.clone());
        }
        for n in ["HEAD".to_owned(), "refs/stash".to_owned()] {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        if let Some(h) = self.head_ref()? {
            let h = h.as_bstr().to_string();
            if !names.contains(&h) {
                names.push(h);
            }
        }
        let before: Vec<Option<ObjectId>> = names
            .iter()
            .map(|n| self.ref_value(n))
            .collect::<Result<_>>()?;
        let head_before = self.head_ref()?.map(|n| n.as_bstr().to_string());
        let config_before = self.config_text()?;
        let stash_before = self.stash_log_text()?;
        let worktree_before = match effect.worktree {
            Some(_) => {
                let id = snapshot_commit(
                    &whole,
                    &format!(
                        "{}: working files before {}",
                        self.product().name,
                        action_tag(action)
                    ),
                )?;
                if effect.backup_worktree {
                    run.backup(self, "worktree", Some(id))?;
                }
                Some(id)
            }
            None => None,
        };
        for (name, b, _) in &effect.refs {
            let short_name = name
                .strip_prefix("refs/heads/")
                .or_else(|| name.strip_prefix("refs/tags/"))
                .unwrap_or(name);
            run.backup(self, short_name, *b)?;
        }

        // The refs, then the configuration, the working files and the index.
        let message = self.reflog_message(&format!("{} (accepted dry run)", action_tag(action)));
        let mut moved: Vec<&(String, Option<ObjectId>, Option<ObjectId>)> = Vec::new();
        let roll_back = |moved: &[&(String, Option<ObjectId>, Option<ObjectId>)]| {
            for (name, b, a) in moved.iter().rev() {
                let _ = self.put_ref(name, *b, *a, "dry run not accepted");
            }
        };
        for r in &effect.refs {
            if let Err(e) = self.put_ref(&r.0, r.2, r.1, &message) {
                roll_back(&moved);
                return Err(e);
            }
            moved.push(r);
        }
        if let Some(text) = &effect.config {
            self.write_config_text(text)?;
        }
        let after_repo = self.reopen()?;
        let after_whole = after_repo.whole()?;
        if let Some((from, to)) = effect.worktree
            && let Err(e) = after_whole.move_worktree(from, to, "accept the dry run")
        {
            roll_back(&moved);
            if effect.config.is_some() {
                let _ = self.write_config_text(&config_before);
            }
            return Err(e);
        }
        match &effect.index {
            IndexEffect::Keep => {}
            IndexEffect::Tree(tree) => {
                let old = after_whole.load_index()?;
                let mut index = after_whole.reindex(*tree, &[&old])?;
                after_whole.write_index(&mut index)?;
            }
            IndexEffect::Paths(updates) => {
                let mut index = after_whole.load_index()?;
                for (path, _) in updates {
                    index.remove_entries(|_, p, _| p == path.as_bstr());
                }
                for (path, object) in updates {
                    if let Some((id, kind)) = object {
                        let stat = stat_of(&crate::worktree::fs_path(
                            &after_whole.workdir,
                            path.as_bstr(),
                        )?);
                        index.dangerously_push_entry(
                            stat,
                            *id,
                            Flags::empty(),
                            index_mode(*kind),
                            path.as_bstr(),
                        );
                    }
                }
                after_whole.write_index(&mut index)?;
            }
        }

        // The journal entry, as `Repo::apply` writes it.
        let after_repo = self.reopen()?;
        let mut refs = Vec::new();
        for (name, b) in names.iter().zip(&before) {
            let a = after_repo.ref_value(name)?;
            if a != *b {
                refs.push(GitRefMove {
                    name: name.clone(),
                    before: oid(b),
                    after: oid(&a),
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
            underway: false,
            ..Restore::default()
        };
        if let Some(b) = worktree_before {
            let after = snapshot_tree(&after_repo.whole()?)?;
            restore.worktree = Some((b.to_string(), after.to_string()));
        }
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let entry = GitUndoEntry {
            id: format!("{time}-{nanos:09}-d{seq}"),
            action: action_tag(action).to_owned(),
            summary: effect.message.clone(),
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
            message: effect.message.clone(),
            undo: Some(entry),
            outcome: effect.outcome.clone(),
            reload: effect.worktree.is_some(),
        })
    }

    /// A tree of a dry run, flattened (its trees may only be in the dry run's memory).
    fn dry_flat(&self, outcome: &DryRunOutcome, tree: ObjectId) -> Result<Flat> {
        let mut repo = self.repo.clone().with_object_memory();
        let mut storage = gix::odb::memory::Storage::default();
        for (id, kind, data) in &outcome.objects {
            if *kind == gix::objs::Kind::Tree {
                storage.insert(*id, (*kind, data.clone()));
            }
        }
        repo.objects.set_object_memory(storage);
        flatten(&repo, tree)
    }
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Whether [`Repo::dry_run`] computes `action` itself; every other action is answered with its
/// plan and accepted by running it.
pub fn dry_run_supported(action: &GitAction) -> bool {
    matches!(
        action,
        GitAction::Merge { .. }
            | GitAction::Pull
            | GitAction::CherryPick { .. }
            | GitAction::Revert { .. }
            | GitAction::Reset { .. }
            | GitAction::Amend { .. }
            | GitAction::Rewrite { .. }
            | GitAction::DeleteBranch { .. }
            | GitAction::Commit { .. }
    )
}

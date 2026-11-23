// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! "Park changes" (wave 14, strand G): a real Git stash, so `git stash list` sees it.
//!
//! Layout as Git writes it: the stash commit `W` holds the tracked files as they were, with
//! parents `HEAD`, `I` (the index) and, when there were untracked files, `U` (a root commit of
//! just those). `refs/stash` points to the newest; `stash@{n}` is the n-th newest entry of
//! its reflog (`logs/refs/stash`). Dropping an entry rewrites the reflog like
//! `git reflog delete --rewrite --updateref`.
//!
//! Untracked files are parked too (new requests are untracked files): parking makes the
//! files match the last commit. Bringing changes back onto another commit than the one they
//! were parked on merges them; when that would conflict, the plan says so and points to the
//! command line.

use std::collections::BTreeMap;

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::merge::tree::TreatAsUnresolved;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};
use rt_app::git::GitFileState;
use rt_app::git::actions::{GitBlockReason, GitPlan, GitRefMove, GitStash};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural_s, short, slash};
use crate::repo::Repo;
use crate::undo::{tree_with, write_commit};
use crate::util::{Blob, Context, Flat, Result, fail, flatten};
use crate::worktree::{FileState, Probe, same};

const STASH: &str = "refs/stash";

/// One line of a reflog.
#[derive(Debug, Clone)]
struct LogLine {
    old: ObjectId,
    new: ObjectId,
    /// Committer, time and zone, then a tab and the message.
    rest: String,
}

impl LogLine {
    fn parse(line: &str) -> Option<Self> {
        let (old, rest) = line.split_once(' ')?;
        let (new, rest) = rest.split_once(' ')?;
        Some(Self {
            old: ObjectId::from_hex(old.as_bytes()).ok()?,
            new: ObjectId::from_hex(new.as_bytes()).ok()?,
            rest: rest.to_owned(),
        })
    }

    fn message(&self) -> &str {
        self.rest.split_once('\t').map(|(_, m)| m).unwrap_or("")
    }

    fn render(&self) -> String {
        format!("{} {} {}\n", self.old, self.new, self.rest)
    }
}

/// `On main: text` → (`main`, `text`); `WIP on main: …` → (`main`, the whole message).
fn parse_message(message: &str) -> (Option<String>, String) {
    let branch_of = |rest: &str| {
        rest.split_once(": ")
            .map(|(b, m)| (b.to_owned(), m.to_owned()))
    };
    let named = |b: String| (b != "(no branch)").then_some(b);
    if let Some(rest) = message.strip_prefix("On ")
        && let Some((b, m)) = branch_of(rest)
    {
        return (named(b), m);
    }
    if let Some(rest) = message.strip_prefix("WIP on ")
        && let Some((b, _)) = branch_of(rest)
    {
        return (named(b), message.to_owned());
    }
    (None, message.to_owned())
}

/// What bringing back a parked change does to the files: repository path → new content
/// (`None`: delete).
type Changes = BTreeMap<BString, Option<Blob>>;

impl Repo {
    fn stash_log_path(&self) -> std::path::PathBuf {
        self.repo
            .common_dir()
            .join("logs")
            .join("refs")
            .join("stash")
    }

    /// The stash's reflog as text (`""` without stashes), for the journal.
    pub(crate) fn stash_log_text(&self) -> Result<String> {
        match std::fs::read_to_string(self.stash_log_path()) {
            Ok(t) => Ok(t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(fail(format!("cannot read the parked changes: {e}"))),
        }
    }

    fn stash_lines(&self) -> Result<Vec<LogLine>> {
        Ok(self
            .stash_log_text()?
            .lines()
            .filter_map(LogLine::parse)
            .collect())
    }

    /// Sets the stash list to `lines` (oldest first): `refs/stash` to the newest, the reflog
    /// to exactly these lines.
    fn write_stash_lines(&self, lines: &[LogLine]) -> Result<()> {
        let text: String = lines.iter().map(LogLine::render).collect();
        self.restore_stash_log(&text)
    }

    /// Puts the stash list back to the reflog `text` (undo).
    pub(crate) fn restore_stash_log(&self, text: &str) -> Result<()> {
        let lines: Vec<LogLine> = text.lines().filter_map(LogLine::parse).collect();
        let current = self.ref_value(STASH)?;
        let path = self.stash_log_path();
        let io = |e: std::io::Error| fail(format!("cannot write the parked changes: {e}"));
        match lines.last() {
            None => {
                if let Some(cur) = current {
                    self.put_ref(STASH, None, Some(cur), "stash: drop")?;
                }
                match std::fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(io(e)),
                }
                Ok(())
            }
            Some(top) => {
                if current != Some(top.new) {
                    self.repo
                        .reference(STASH, top.new, PreviousValue::Any, "stash")
                        .ctx("cannot update the parked changes")?;
                }
                // The ref update logged a line of its own; the list is exactly `text`.
                std::fs::create_dir_all(path.parent().expect("has a parent")).map_err(io)?;
                let tmp = path.with_extension("rt-tmp");
                std::fs::write(&tmp, text).map_err(io)?;
                std::fs::rename(&tmp, &path).map_err(io)
            }
        }
    }

    /// The parked changes, newest first (`stash@{0}` first).
    pub fn stashes(&self) -> Result<Vec<GitStash>> {
        let lines = self.stash_lines()?;
        let mut out = Vec::new();
        for (index, line) in lines.iter().rev().enumerate() {
            let (branch, message) = parse_message(line.message());
            let commit = self.repo.find_commit(line.new).ctx("cannot read a stash")?;
            let time = commit.time().map(|t| t.seconds).unwrap_or(0);
            let parents: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
            let w = flatten(&self.repo, self.commit_tree(line.new)?)?;
            let base = self.flat_of(parents.first().copied())?;
            let mut files: Vec<String> = crate::branch_actions::diff_paths(&base, &w);
            if let Some(u) = parents.get(2) {
                files.extend(self.flat_of(Some(*u))?.keys().map(|p| slash(p)));
            }
            files.sort();
            files.dedup();
            out.push(GitStash {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                oid: line.new.to_string(),
                message,
                time,
                branch,
                files,
            });
        }
        Ok(out)
    }

    // ---- stash -----------------------------------------------------------------------

    pub(crate) fn plan_stash(&self, cx: &Ctx, mut p: GitPlan) -> Result<GitPlan> {
        let files: Vec<String> = cx
            .changes()?
            .iter()
            .filter(|f| f.state != GitFileState::Conflicted)
            .map(|f| format!("/{}", f.path))
            .collect();
        p.stop(cx.in_progress()?);
        if cx.head.is_none() {
            p.stop(Some(block(
                GitBlockReason::NothingToDo,
                "Make the first commit first.",
            )));
        }
        if files.is_empty() {
            p.stop(Some(block(
                GitBlockReason::NothingToDo,
                "There are no changes to park.",
            )));
        }
        p.stop(cx.unsaved());
        p.steps.push(format!(
            "Parks {} and makes your files match the last commit. Bring them back any time.",
            plural_s(files.len(), "change")
        ));
        p.refs.push(GitRefMove {
            name: STASH.into(),
            before: self.ref_value(STASH)?.map(|id| id.to_string()),
            after: Some(String::new()),
        });
        p.files = files;
        Ok(p)
    }

    pub(crate) fn run_stash(&self, cx: &Ctx, message: Option<&str>) -> Result<Done> {
        let head = cx
            .head
            .ok_or_else(|| fail("Make the first commit first."))?;
        let whole = &cx.whole;
        let label = cx.cur.clone().unwrap_or_else(|| "(no branch)".into());
        let identity = self.config_identity();
        let changes = cx.changes()?;
        let tracked: Vec<_> = changes
            .iter()
            .filter(|f| f.state != GitFileState::Untracked)
            .collect();
        let untracked: Vec<_> = changes
            .iter()
            .filter(|f| f.state == GitFileState::Untracked)
            .collect();
        let count = tracked.len() + untracked.len();
        let head_tree = whole.head_tree()?;
        let subject = format!("{} {}", short(&head), self.subject(head));
        let index = whole.load_index()?;
        let i_tree = whole.tree_from_index(&index)?;
        let i = write_commit(
            whole,
            i_tree,
            vec![head],
            &format!("index on {label}: {subject}\n"),
            identity.as_ref(),
        )?;
        let w_tree = tree_with(whole, head_tree, &tracked)?;
        let mut parents = vec![head, i];
        if !untracked.is_empty() {
            let empty = ObjectId::empty_tree(self.repo.object_hash());
            let u_tree = tree_with(whole, empty, &untracked)?;
            parents.push(write_commit(
                whole,
                u_tree,
                Vec::new(),
                &format!("untracked files on {label}: {subject}\n"),
                identity.as_ref(),
            )?);
        }
        let text = message
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Parked on {}", cx.cur.as_deref().unwrap_or("HEAD")));
        let w_message = format!("On {label}: {text}");
        let w = write_commit(
            whole,
            w_tree,
            parents,
            &format!("{w_message}\n"),
            identity.as_ref(),
        )?;
        let all: Vec<_> = tracked.iter().chain(untracked.iter()).copied().collect();
        let now = tree_with(whole, head_tree, &all)?;
        self.repo
            .edit_reference(RefEdit {
                change: Change::Update {
                    log: LogChange {
                        mode: RefLog::AndReference,
                        force_create_reflog: true,
                        message: w_message.as_str().into(),
                    },
                    expected: PreviousValue::Any,
                    new: Target::Object(w),
                },
                name: FullName::try_from(STASH).expect("valid"),
                deref: false,
            })
            .ctx("cannot park the changes")?;
        whole.move_worktree(now, head_tree, "park changes")?;
        Ok(Done::new(
            format!("Parked {}.", plural_s(count, "change")),
            true,
        ))
    }

    // ---- unstash, drop -------------------------------------------------------------------

    /// The parked change `index` and its log line (`None` if there is none).
    fn stash_at(&self, index: u32) -> Result<Option<(ObjectId, Vec<LogLine>)>> {
        let lines = self.stash_lines()?;
        let at = lines.len().checked_sub(1 + index as usize);
        Ok(at.map(|i| (lines[i].new, lines)))
    }

    /// The value `refs/stash` has after removing entry `index`.
    fn stash_after_drop(lines: &[LogLine], index: u32) -> Option<ObjectId> {
        let n = lines.len();
        if index == 0 {
            n.checked_sub(2).map(|i| lines[i].new)
        } else {
            lines.last().map(|l| l.new)
        }
    }

    /// What bringing back `w` changes: its tracked changes (merged onto `HEAD` when it moved
    /// since) and its untracked files. `Err(conflicts)` when the merge would conflict.
    fn unstash_changes(
        &self,
        repo: &gix::Repository,
        cx: &Ctx,
        w: ObjectId,
    ) -> Result<std::result::Result<Changes, Vec<String>>> {
        let commit = self.repo.find_commit(w).ctx("cannot read a stash")?;
        let parents: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
        let base = *parents
            .first()
            .ok_or_else(|| fail("this stash has no parent"))?;
        let w_tree = self.commit_tree(w)?;
        let mut changes = Changes::new();
        let (from, to): (Flat, Flat) = if cx.head == Some(base) {
            (self.flat_of(Some(base))?, flatten(&self.repo, w_tree)?)
        } else {
            let head_tree = cx.whole.head_tree()?;
            let labels = gix::merge::blob::builtin_driver::text::Labels {
                ancestor: None,
                current: None,
                other: None,
            };
            let options = repo
                .tree_merge_options()
                .ctx("cannot read the merge configuration")?;
            let mut outcome = repo
                .merge_trees(self.commit_tree(base)?, head_tree, w_tree, labels, options)
                .map_err(|e| fail(format!("cannot merge the parked changes: {e}")))?;
            if outcome.has_unresolved_conflicts(TreatAsUnresolved::git()) {
                let mut paths: Vec<String> = outcome
                    .conflicts
                    .iter()
                    .map(|c| slash(c.ours.location()))
                    .collect();
                paths.sort();
                paths.dedup();
                return Ok(Err(paths));
            }
            let merged = outcome
                .tree
                .write()
                .ctx("cannot merge the parked changes")?;
            (
                flatten(repo, head_tree)?,
                flatten(merged.repo, merged.detach())?,
            )
        };
        for p in from.keys().chain(to.keys()) {
            if from.get(p) != to.get(p) {
                changes.insert(p.clone(), to.get(p).copied());
            }
        }
        if let Some(u) = parents.get(2) {
            for (p, blob) in self.flat_of(Some(*u))? {
                changes.insert(p, Some(blob));
            }
        }
        Ok(Ok(changes))
    }

    pub(crate) fn plan_unstash(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        index: u32,
        keep: bool,
    ) -> Result<GitPlan> {
        let Some((w, lines)) = self.stash_at(index)? else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                "There are no such parked changes.",
            )));
        };
        let stash = self
            .stashes()?
            .into_iter()
            .nth(index as usize)
            .ok_or_else(|| fail("There are no such parked changes."))?;
        p.steps.push(format!(
            "Brings back \"{}\" ({}).",
            stash.message,
            plural_s(stash.files.len(), "file")
        ));
        if !keep {
            p.steps
                .push("Removes it from the parked changes; a backup keeps it for Undo.".into());
            p.refs.push(GitRefMove {
                name: STASH.into(),
                before: lines.last().map(|l| l.new.to_string()),
                after: Self::stash_after_drop(&lines, index).map(|id| id.to_string()),
            });
        }
        p.backup = true;
        p.stop(cx.unsaved());
        p.stop(cx.in_progress()?);
        let mem = self.repo.clone().with_object_memory();
        match self.unstash_changes(&mem, cx, w)? {
            Err(conflicts) => {
                p.files = stash.files;
                p.stop(Some(block(
                    GitBlockReason::Unsupported,
                    format!(
                        "These parked changes overlap with commits made since ({}). Switch to {} first, or bring them back on the command line (git stash apply).",
                        conflicts.join(", "),
                        stash.branch.as_deref().unwrap_or("the commit they were parked on")
                    ),
                )));
            }
            Ok(changes) => {
                p.files = changes.keys().map(|k| slash(k)).collect();
                let head = self.flat_of(cx.head)?;
                let index_state = cx.whole.load_index()?;
                let mut probe = Probe::new(&cx.whole)?;
                let mut clash = Vec::new();
                for (path, want) in &changes {
                    let now = match probe.state(path.as_bstr(), &index_state)? {
                        FileState::File(b) => Some(b),
                        _ => None,
                    };
                    let eq = |a: Option<Blob>, b: Option<Blob>| match (a, b) {
                        (Some(a), Some(b)) => same(a, b),
                        (None, None) => true,
                        _ => false,
                    };
                    if !eq(now, head.get(path).copied()) && !eq(now, *want) {
                        clash.push(slash(path));
                    }
                }
                if !clash.is_empty() {
                    p.stop(Some(block(
                        GitBlockReason::LocalChanges,
                        format!(
                            "Your changes to {} would be overwritten. Commit or park them first.",
                            clash.join(", ")
                        ),
                    )));
                }
            }
        }
        Ok(p)
    }

    /// Removes entry `index` from the stash list.
    fn drop_stash(&self, index: u32) -> Result<()> {
        let mut lines = self.stash_lines()?;
        let Some(at) = lines.len().checked_sub(1 + index as usize) else {
            return Err(fail("There are no such parked changes."));
        };
        lines.remove(at);
        // The next newer entry now follows the next older one (`--rewrite`).
        if at < lines.len() {
            let zero = ObjectId::null(self.repo.object_hash());
            lines[at].old = if at == 0 { zero } else { lines[at - 1].new };
        }
        self.write_stash_lines(&lines)
    }

    pub(crate) fn run_unstash(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        index: u32,
        keep: bool,
    ) -> Result<Done> {
        let (w, _) = self
            .stash_at(index)?
            .ok_or_else(|| fail("There are no such parked changes."))?;
        let message = self
            .stashes()?
            .into_iter()
            .nth(index as usize)
            .map(|s| s.message)
            .unwrap_or_default();
        if !keep {
            run.backup(self, "stash", Some(w))?;
        }
        let changes = self
            .unstash_changes(&self.repo, cx, w)?
            .map_err(|_| fail("These parked changes overlap with commits made since."))?;
        let now = crate::undo::snapshot_tree(&cx.whole)?;
        let mut editor = self.repo.edit_tree(now).ctx("cannot build the tree")?;
        for (path, want) in &changes {
            match want {
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
        cx.whole
            .move_worktree(now, want, "bring back parked changes")?;
        if !keep {
            self.drop_stash(index)?;
        }
        Ok(Done::new(format!("Brought back \"{message}\"."), true))
    }

    pub(crate) fn plan_drop_stash(&self, mut p: GitPlan, index: u32) -> Result<GitPlan> {
        let Some((_, lines)) = self.stash_at(index)? else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                "There are no such parked changes.",
            )));
        };
        let message = self
            .stashes()?
            .into_iter()
            .nth(index as usize)
            .map(|s| s.message)
            .unwrap_or_default();
        p.steps
            .push(format!("Deletes the parked changes \"{message}\"."));
        p.refs.push(GitRefMove {
            name: STASH.into(),
            before: lines.last().map(|l| l.new.to_string()),
            after: Self::stash_after_drop(&lines, index).map(|id| id.to_string()),
        });
        p.backup = true;
        p.destructive = true;
        Ok(p)
    }

    pub(crate) fn run_drop_stash(&self, run: &mut Run<'_>, index: u32) -> Result<Done> {
        let (w, _) = self
            .stash_at(index)?
            .ok_or_else(|| fail("There are no such parked changes."))?;
        let message = self
            .stashes()?
            .into_iter()
            .nth(index as usize)
            .map(|s| s.message)
            .unwrap_or_default();
        run.backup(self, "stash", Some(w))?;
        self.drop_stash(index)?;
        Ok(Done::new(
            format!("Deleted the parked changes \"{message}\"."),
            false,
        ))
    }
}

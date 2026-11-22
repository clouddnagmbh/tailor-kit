// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Secret rescue (`rt_app::rescue`): scanning every commit with the secret scanner, and
//! rewriting the history so a found value becomes a `{{variable}}`, with backup refs first.
//!
//! - [`Repo::scan_history`] walks every commit reachable from `HEAD` and the local branches
//!   and hands each commit and each blob of the collection once to
//!   [`rt_app::rescue::HistoryScanner`] (the web app does the same on isomorphic-git).
//!   Backups (`refs/requesttailor/backup/…`), remote-tracking refs and tags are not scanned.
//! - [`Repo::rewrite_history`] refuses during a merge, on a detached `HEAD` and with local
//!   changes in the collection. It reads the value again from the blob of the plan's
//!   findings, writes new blobs (every text file of the repository that holds the value,
//!   inside the collection or not, via [`rt_app::rescue::replace_value`]), trees and commits
//!   for every local branch, hands the value to `store` (the secret store), saves the old
//!   tips under `refs/requesttailor/backup/<unix>/<branch>`, moves the branches and updates
//!   the work tree to the new `HEAD`.
//! - Objects are rewritten byte for byte: a tree keeps its entries (modes as written, names,
//!   order) with only the changed ids; a commit keeps its headers (author, committer,
//!   encoding, others) and message with only `tree` and `parent` changed. Signatures
//!   (`gpgsig`) are dropped: they would no longer verify. Untouched blobs, trees and commits
//!   keep their ids.
//! - No push: the result names the branches with an upstream, which need
//!   `git push --force-with-lease`. Tags are not moved; the result names those that point at
//!   a rewritten commit.
//!
//! Owner: wave 12, strand G.

use std::collections::{BTreeSet, BinaryHeap, HashMap};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::index::entry::Mode;
use gix::refs::Target;
use gix::refs::transaction::PreviousValue;
use rt_app::git::GitFileState;
use rt_app::rescue::{
    self, HistoryScan, HistoryScanner, RescuePlan, RescueResult, RewrittenBranch, ScanProgress,
};
use rt_app::secret_scan;

use crate::commit::describe;
use crate::repo::Repo;
use crate::util::{Context, Result, blob_bytes, fail, flatten};

/// Progress is reported after this many commits (and after the last).
const PROGRESS_EVERY: u32 = 20;

type Graph = HashMap<ObjectId, Vec<ObjectId>>;

/// A raw object for `write_object`.
struct Raw<'a> {
    kind: gix::objs::Kind,
    data: &'a [u8],
}

impl gix::objs::WriteTo for Raw<'_> {
    fn write_to(&self, out: &mut dyn std::io::Write) -> std::io::Result<()> {
        out.write_all(self.data)
    }
    fn kind(&self) -> gix::objs::Kind {
        self.kind
    }
    fn size(&self) -> u64 {
        self.data.len() as u64
    }
}

/// Commits of `parents` (commit → parents; parents outside the map are ignored), parents
/// first; among those ready, the oldest (author time, then id) first.
fn topo_order(parents: &Graph, time: &HashMap<ObjectId, i64>) -> Vec<ObjectId> {
    let mut children: Graph = HashMap::new();
    let mut waiting: HashMap<ObjectId, usize> = HashMap::new();
    for (id, ps) in parents {
        let ps: BTreeSet<ObjectId> = ps
            .iter()
            .copied()
            .filter(|p| parents.contains_key(p))
            .collect();
        waiting.insert(*id, ps.len());
        for p in ps {
            children.entry(p).or_default().push(*id);
        }
    }
    let key = |id: ObjectId| std::cmp::Reverse((time.get(&id).copied().unwrap_or(0), id));
    let mut ready: BinaryHeap<_> = waiting
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| key(*id))
        .collect();
    let mut out = Vec::with_capacity(parents.len());
    while let Some(std::cmp::Reverse((_, id))) = ready.pop() {
        out.push(id);
        for child in children.get(&id).into_iter().flatten() {
            let n = waiting.get_mut(child).expect("known");
            *n -= 1;
            if *n == 0 {
                ready.push(key(*child));
            }
        }
    }
    out
}

/// A raw commit with `tree` and `parents` in place of its own, every other header line
/// (continuation lines included) and the message as they are; signatures dropped.
fn recommit(data: &[u8], tree: ObjectId, parents: &[ObjectId]) -> Result<Vec<u8>> {
    let end = data
        .find(b"\n\n")
        .ok_or_else(|| fail("cannot read a commit: unexpected format"))?;
    let (head, message) = (&data[..=end], &data[end + 1..]);
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(format!("tree {tree}\n").as_bytes());
    for p in parents {
        out.extend_from_slice(format!("parent {p}\n").as_bytes());
    }
    let mut skip = false;
    for line in head.lines_with_terminator() {
        if line.first() != Some(&b' ') {
            let name = line.split(|b| *b == b' ').next().unwrap_or_default();
            skip = matches!(name, b"tree" | b"parent" | b"gpgsig" | b"gpgsig-sha256");
        }
        if !skip {
            out.extend_from_slice(line);
        }
    }
    out.extend_from_slice(message);
    Ok(out)
}

/// New objects for the rewrite, each old object once.
struct Rewriter<'r> {
    repo: &'r gix::Repository,
    value: String,
    variable: String,
    blobs: HashMap<ObjectId, ObjectId>,
    trees: HashMap<ObjectId, ObjectId>,
}

impl Rewriter<'_> {
    fn write(&self, kind: gix::objs::Kind, data: &[u8]) -> Result<ObjectId> {
        Ok(self
            .repo
            .write_object(Raw { kind, data })
            .ctx("cannot write to the repository")?
            .detach())
    }

    fn data(&self, id: ObjectId) -> Result<Vec<u8>> {
        Ok(self
            .repo
            .find_object(id)
            .ctx("cannot read from the repository")?
            .detach()
            .data)
    }

    fn blob(&mut self, id: ObjectId) -> Result<ObjectId> {
        if let Some(new) = self.blobs.get(&id) {
            return Ok(*new);
        }
        let bytes = blob_bytes(self.repo, id)?;
        let new = match secret_scan::text_of(&bytes) {
            Some(text) if rescue::contains_value(text, &self.value) => {
                let replaced = rescue::replace_value(text, &self.value, &self.variable);
                self.write(gix::objs::Kind::Blob, replaced.as_bytes())?
            }
            _ => id,
        };
        self.blobs.insert(id, new);
        Ok(new)
    }

    fn tree(&mut self, id: ObjectId) -> Result<ObjectId> {
        if let Some(new) = self.trees.get(&id) {
            return Ok(*new);
        }
        let data = self.data(id)?;
        let hash_len = id.as_slice().len();
        let bad = || fail("cannot read a tree: unexpected format");
        let mut out = Vec::with_capacity(data.len());
        let mut changed = false;
        let mut at = 0;
        while at < data.len() {
            let space = data[at..].find_byte(b' ').ok_or_else(bad)? + at;
            let nul = data[space..].find_byte(0).ok_or_else(bad)? + space;
            let oid_end = nul + 1 + hash_len;
            if oid_end > data.len() {
                return Err(bad());
            }
            let oid = ObjectId::try_from(&data[nul + 1..oid_end]).map_err(|_| bad())?;
            let new = match &data[at..space] {
                b"40000" | b"040000" => self.tree(oid)?,
                mode if mode.starts_with(b"100") => self.blob(oid)?,
                // Symbolic links and submodules stay.
                _ => oid,
            };
            changed |= new != oid;
            out.extend_from_slice(&data[at..=nul]);
            out.extend_from_slice(new.as_slice());
            at = oid_end;
        }
        let new = if changed {
            self.write(gix::objs::Kind::Tree, &out)?
        } else {
            id
        };
        self.trees.insert(id, new);
        Ok(new)
    }

    fn commit(&self, id: ObjectId, tree: ObjectId, parents: &[ObjectId]) -> Result<ObjectId> {
        let data = self.data(id)?;
        let out = recommit(&data, tree, parents)?;
        if out == data {
            return Ok(id);
        }
        self.write(gix::objs::Kind::Commit, &out)
    }
}

impl Repo {
    /// Local branches (short name, tip), sorted by name.
    fn branch_tips(&self) -> Result<Vec<(String, ObjectId)>> {
        let refs = self.repo.references().ctx("cannot list branches")?;
        let mut out = Vec::new();
        for r in refs.local_branches().ctx("cannot list branches")? {
            let mut r = r.map_err(|e| fail(format!("cannot list branches: {e}")))?;
            let name = r.name().shorten().to_str_lossy().into_owned();
            let id = r.peel_to_commit().ctx("cannot read a branch")?.id;
            out.push((name, id));
        }
        out.sort();
        Ok(out)
    }

    /// Every commit reachable from `tips`, with its parents and author time.
    fn graph(&self, tips: &[ObjectId]) -> Result<(Graph, HashMap<ObjectId, i64>)> {
        let mut parents = HashMap::new();
        let mut time = HashMap::new();
        if tips.is_empty() {
            return Ok((parents, time));
        }
        let walk = self
            .repo
            .rev_walk(tips.iter().copied())
            .all()
            .ctx("cannot walk the history")?;
        for info in walk {
            let info = info.ctx("cannot walk the history")?;
            let commit = self.repo.find_commit(info.id).ctx("cannot read a commit")?;
            let ps: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
            let t = commit.author().map(|a| a.seconds()).unwrap_or_default();
            parents.insert(info.id, ps);
            time.insert(info.id, t);
        }
        Ok((parents, time))
    }

    /// Scans every commit of `HEAD` and the local branches with the secret scanner; one
    /// finding per value (see `rt_app::rescue`). `progress` hears about every few commits.
    pub fn scan_history(&self, progress: &mut dyn FnMut(ScanProgress)) -> Result<HistoryScan> {
        let branches = self.branch_tips()?;
        let head = self.head_commit()?;
        let mut tips: Vec<ObjectId> = branches.iter().map(|(_, id)| *id).collect();
        tips.extend(head);
        tips.sort();
        tips.dedup();
        let (parents, _) = self.graph(&tips)?;
        let total = u32::try_from(parents.len()).unwrap_or(u32::MAX);
        progress(ScanProgress { scanned: 0, total });
        let mut scanner = HistoryScanner::new();
        let mut ids: Vec<&ObjectId> = parents.keys().collect();
        ids.sort();
        for (n, id) in ids.into_iter().enumerate() {
            let mut files = Vec::new();
            for (path, blob) in flatten(&self.repo, self.commit_tree(*id)?)? {
                if !matches!(blob.mode, Mode::FILE | Mode::FILE_EXECUTABLE) {
                    continue;
                }
                let Some(rel) = self.scope.rel(path.as_bstr()) else {
                    continue;
                };
                let blob_id = blob.id.to_string();
                if !scanner.knows(&blob_id) {
                    let bytes = blob_bytes(&self.repo, blob.id)?;
                    scanner.add_blob(&blob_id, &rel, secret_scan::text_of(&bytes));
                }
                files.push((rel, blob_id));
            }
            let commit_parents = parents[id].iter().map(ObjectId::to_string).collect();
            scanner.add_commit(describe(&self.repo, *id)?, commit_parents, files);
            let scanned = u32::try_from(n + 1).unwrap_or(u32::MAX);
            if scanned % PROGRESS_EVERY == 0 || scanned == total {
                progress(ScanProgress { scanned, total });
            }
        }
        let names: Vec<(String, String)> = branches
            .iter()
            .map(|(name, id)| (name.clone(), id.to_string()))
            .collect();
        let head = head.map(|h| h.to_string());
        Ok(scanner.finish(&names, head.as_deref()))
    }

    /// Text of the collection file `path` in `commit`, `None` if it is not there.
    fn text_at(&self, commit: &str, path: &str) -> Result<Option<String>> {
        let id = ObjectId::from_hex(commit.as_bytes()).map_err(|_| fail("invalid commit id"))?;
        if self.repo.find_commit(id).is_err() {
            return Ok(None);
        }
        let Some((mode, blob)) = self.entry_at(id, &self.scope.abs(path)?)? else {
            return Ok(None);
        };
        if !matches!(mode, Mode::FILE | Mode::FILE_EXECUTABLE) {
            return Ok(None);
        }
        let bytes = blob_bytes(&self.repo, blob)?;
        Ok(secret_scan::text_of(&bytes).map(str::to_owned))
    }

    /// The upstream remote of local branch `name`, if the branch was pushed or fetched.
    fn pushed_to(&self, name: &str) -> Result<Option<String>> {
        let (Some(remote), Some(merge)) = (
            self.config(&format!("branch.{name}.remote")),
            self.config(&format!("branch.{name}.merge")),
        ) else {
            return Ok(None);
        };
        let Some(short) = merge.strip_prefix("refs/heads/") else {
            return Ok(None);
        };
        if remote == "." {
            return Ok(None);
        }
        Ok(self
            .ref_commit(&format!("refs/remotes/{remote}/{short}"))?
            .map(|_| remote))
    }

    /// Tags that point at a commit `map` rewrote.
    fn moved_tags(&self, map: &HashMap<ObjectId, ObjectId>) -> Result<Vec<String>> {
        let refs = self.repo.references().ctx("cannot list tags")?;
        let mut tags = Vec::new();
        for r in refs.tags().ctx("cannot list tags")? {
            let Ok(mut r) = r else { continue };
            let name = r.name().shorten().to_str_lossy().into_owned();
            if let Ok(commit) = r.peel_to_commit()
                && map.get(&commit.id).is_some_and(|new| *new != commit.id)
            {
                tags.push(name);
            }
        }
        tags.sort();
        Ok(tags)
    }

    /// Rewrites the local branches so the value of `plan`'s findings becomes
    /// `{{plan.variable}}` (see the module documentation). Call it only after the user
    /// confirmed. `store` gets the value before any ref changes; `now` (Unix seconds) names
    /// the backups.
    pub fn rewrite_history(
        &self,
        plan: &RescuePlan,
        now: i64,
        store: &mut dyn FnMut(&str) -> Result<()>,
    ) -> Result<RescueResult> {
        rescue::check_plan(plan)?;
        if self.merge_head()?.is_some() {
            return Err(fail("finish the merge before rewriting the history"));
        }
        if self.head_ref()?.is_none() {
            return Err(fail("switch to a branch before rewriting the history"));
        }
        if let Some(file) = self
            .status()?
            .files
            .into_iter()
            .find(|f| f.state != GitFileState::Untracked)
        {
            return Err(fail(format!(
                "commit or discard your changes before rewriting the history ({})",
                file.path
            )));
        }
        let value = rescue::plan_value(plan, |at| self.text_at(&at.commit, &at.path))?;

        // New objects first; nothing points at them yet.
        let branches = self.branch_tips()?;
        let tips: Vec<ObjectId> = branches.iter().map(|(_, id)| *id).collect();
        let (parents, time) = self.graph(&tips)?;
        let mut rewriter = Rewriter {
            repo: &self.repo,
            value,
            variable: plan.variable.clone(),
            blobs: HashMap::new(),
            trees: HashMap::new(),
        };
        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let mut rewritten = 0u32;
        for id in topo_order(&parents, &time) {
            let tree = self.commit_tree(id)?;
            let new_tree = rewriter.tree(tree)?;
            let old_parents = &parents[&id];
            let new_parents: Vec<ObjectId> = old_parents
                .iter()
                .map(|p| map.get(p).copied().unwrap_or(*p))
                .collect();
            let new = if new_tree == tree && new_parents == *old_parents {
                id
            } else {
                rewriter.commit(id, new_tree, &new_parents)?
            };
            if new != id {
                rewritten += 1;
            }
            map.insert(id, new);
        }
        let changed: Vec<(String, ObjectId, ObjectId)> = branches
            .iter()
            .filter_map(|(name, old)| {
                let new = map.get(old).copied().unwrap_or(*old);
                (new != *old).then(|| (name.clone(), *old, new))
            })
            .collect();
        if changed.is_empty() {
            return Err(fail(
                "the value is not in the history of any local branch: scan the history again",
            ));
        }

        // The work tree must be able to follow before any ref changes.
        let head_old = self.head_commit()?;
        let switch = match head_old.map(|h| (h, map.get(&h).copied().unwrap_or(h))) {
            Some((old, new)) if old != new => {
                Some((self.commit_tree(old)?, self.commit_tree(new)?))
            }
            _ => None,
        };
        if let Some((from, to)) = switch {
            let index = self.load_index()?;
            let blocked = crate::worktree::blocked(
                self,
                &flatten(&self.repo, from)?,
                &flatten(&self.repo, to)?,
                &index,
            )?;
            if !blocked.is_empty() {
                return Err(crate::worktree::blocked_error(
                    self,
                    "rewrite the history",
                    &blocked,
                ));
            }
        }

        store(&rewriter.value)?;

        // Backups, under a prefix no earlier rescue used.
        let mut unix = now;
        let taken = |unix: i64| -> Result<bool> {
            for (name, _, _) in &changed {
                let full = format!("{}{name}", rescue::backup_prefix(unix));
                if self.ref_commit(&full)?.is_some() {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        while taken(unix)? {
            unix += 1;
        }
        let backup = rescue::backup_prefix(unix);
        for (name, old, _) in &changed {
            self.repo
                .reference(
                    format!("{backup}{name}"),
                    *old,
                    PreviousValue::MustNotExist,
                    "requesttailor: backup before secret rescue",
                )
                .ctx("cannot write the backup ref")?;
        }
        let message = format!(
            "requesttailor: secret rescue, value replaced by {{{{{}}}}}",
            plan.variable
        );
        for (name, old, new) in &changed {
            self.repo
                .reference(
                    format!("refs/heads/{name}"),
                    *new,
                    PreviousValue::MustExistAndMatch(Target::Object(*old)),
                    message.as_str(),
                )
                .ctx("cannot update the branch")?;
        }
        if let Some((from, to)) = switch {
            let mut index = crate::worktree::switch(self, from, to, "rewrite the history")?;
            self.write_index(&mut index)?;
        }

        let tags = self.moved_tags(&map)?;
        let mut out = Vec::new();
        for (name, old, new) in changed {
            let remote = self.pushed_to(&name)?;
            out.push(RewrittenBranch {
                pushed: remote.is_some(),
                remote,
                old: old.to_string(),
                new: new.to_string(),
                name,
            });
        }
        Ok(RescueResult {
            rewritten,
            branches: out,
            backup,
            tags,
        })
    }
}

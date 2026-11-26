// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Walking and rewriting the whole history: the building blocks of a secret rescue (wave 12,
//! strand G). The product decides what to look for and what to replace; the core walks, writes
//! the objects and moves the refs, with backups first.
//!
//! - [`Repo::walk_history`] walks every commit reachable from `HEAD` and the local branches
//!   and hands each commit and each blob of the scope once to a [`HistoryVisitor`]. Backups
//!   (`refs/<namespace>/backup/…`), remote-tracking refs and tags are not walked.
//! - [`Repo::rewrite_blobs`] refuses during a merge, on a detached `HEAD` and with local
//!   changes ([`Repo::check_rewrite`]). It writes new blobs (every regular file of the
//!   repository whose content the edit changes, inside the scope or not), trees and commits
//!   for every local branch, calls `before_refs` (e.g. to store a value that leaves the
//!   history), saves the old tips under `refs/<namespace>/backup/<unix>/<branch>`, moves the
//!   branches and updates the work tree to the new `HEAD`.
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
use tk_git_types::{GitCommit, GitFileState};

use crate::commit::describe;
use crate::repo::Repo;
use crate::util::{Context, Result, blob_bytes, fail, flatten};

/// Progress is reported after this many commits (and after the last).
const PROGRESS_EVERY: u32 = 20;

/// What [`Repo::walk_history`] hands over.
pub trait HistoryVisitor {
    /// Whether the blob `id` was handed over already (it is not read again).
    fn knows(&self, id: &str) -> bool;
    /// A regular file of the scope, once per blob, at the first (scope-relative) path seen.
    fn blob(&mut self, id: &str, path: &str, content: &[u8]);
    /// A commit, its parents and its regular files of the scope (path, blob id).
    fn commit(&mut self, commit: GitCommit, parents: Vec<String>, files: Vec<(String, String)>);
}

/// The tips a walk started from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryTips {
    /// Local branches (short name, tip), sorted by name.
    pub branches: Vec<(String, String)>,
    /// The commit `HEAD` points to; `None` without commits.
    pub head: Option<String>,
}

/// How a rewrite names itself: the reflog says `<namespace>: backup before <what>` on the
/// backups and `<namespace>: <what>, <detail>` on the moved branches; `unchanged` is the
/// error when no local branch changes.
#[derive(Debug, Clone, Copy)]
pub struct HistoryEdit<'a> {
    pub what: &'a str,
    pub detail: &'a str,
    pub unchanged: &'a str,
}

/// A local branch a rewrite moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRewrite {
    pub name: String,
    pub old: String,
    pub new: String,
    /// Its upstream's remote (`origin`) when the branch was pushed or fetched: it needs a
    /// force push there.
    pub remote: Option<String>,
}

/// What a rewrite did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRewrite {
    /// Commits written anew.
    pub rewritten: u32,
    pub branches: Vec<BranchRewrite>,
    /// Ref prefix of this rewrite's backups (`refs/<namespace>/backup/<unix>/`).
    pub backup: String,
    /// Tags that point at a rewritten commit (tags are not moved).
    pub tags: Vec<String>,
}

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
struct Rewriter<'r, 'e> {
    repo: &'r gix::Repository,
    edit: &'e mut dyn FnMut(&[u8]) -> Option<Vec<u8>>,
    blobs: HashMap<ObjectId, ObjectId>,
    trees: HashMap<ObjectId, ObjectId>,
}

impl Rewriter<'_, '_> {
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
        let new = match (self.edit)(&bytes) {
            Some(edited) => self.write(gix::objs::Kind::Blob, &edited)?,
            None => id,
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

    /// Walks every commit of `HEAD` and the local branches (in id order) and hands it, and
    /// each regular file of the scope once per blob, to `visitor`. `progress` hears
    /// `(walked, total)` at the start and after every few commits.
    pub fn walk_history(
        &self,
        visitor: &mut dyn HistoryVisitor,
        progress: &mut dyn FnMut(u32, u32),
    ) -> Result<HistoryTips> {
        let branches = self.branch_tips()?;
        let head = self.head_commit()?;
        let mut tips: Vec<ObjectId> = branches.iter().map(|(_, id)| *id).collect();
        tips.extend(head);
        tips.sort();
        tips.dedup();
        let (parents, _) = self.graph(&tips)?;
        let total = u32::try_from(parents.len()).unwrap_or(u32::MAX);
        progress(0, total);
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
                if !visitor.knows(&blob_id) {
                    let bytes = blob_bytes(&self.repo, blob.id)?;
                    visitor.blob(&blob_id, &rel, &bytes);
                }
                files.push((rel, blob_id));
            }
            let commit_parents = parents[id].iter().map(ObjectId::to_string).collect();
            visitor.commit(describe(&self.repo, *id)?, commit_parents, files);
            let walked = u32::try_from(n + 1).unwrap_or(u32::MAX);
            if walked % PROGRESS_EVERY == 0 || walked == total {
                progress(walked, total);
            }
        }
        Ok(HistoryTips {
            branches: branches
                .iter()
                .map(|(name, id)| (name.clone(), id.to_string()))
                .collect(),
            head: head.map(|h| h.to_string()),
        })
    }

    /// The content of the regular file `path` (scope-relative) in `commit` (a full id),
    /// `None` if the commit or the file is not there.
    pub fn file_at(&self, commit: &str, path: &str) -> Result<Option<Vec<u8>>> {
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
        Ok(Some(blob_bytes(&self.repo, blob)?))
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

    /// Whether the history can be rewritten now: no merge in progress, `HEAD` on a branch, no
    /// local changes in the scope (untracked files are fine).
    pub fn check_rewrite(&self) -> Result<()> {
        if self.underway()?.is_some() {
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
        Ok(())
    }

    /// Rewrites the local branches with every blob of their history that `edit` changes
    /// (`Some(new content)`; `None` keeps the blob), see the module documentation. Call it
    /// only after the user confirmed. `before_refs` runs after every new object is written and
    /// the work tree can follow, before any ref changes; `now` (Unix seconds) names the
    /// backups.
    pub fn rewrite_blobs(
        &self,
        edit: &mut dyn FnMut(&[u8]) -> Option<Vec<u8>>,
        what: &HistoryEdit<'_>,
        now: i64,
        before_refs: &mut dyn FnMut() -> Result<()>,
    ) -> Result<HistoryRewrite> {
        self.check_rewrite()?;

        // New objects first; nothing points at them yet.
        let branches = self.branch_tips()?;
        let tips: Vec<ObjectId> = branches.iter().map(|(_, id)| *id).collect();
        let (parents, time) = self.graph(&tips)?;
        let mut rewriter = Rewriter {
            repo: &self.repo,
            edit,
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
            return Err(fail(what.unchanged));
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

        before_refs()?;

        // Backups, under a prefix no earlier rewrite used.
        let mut unix = now;
        let taken = |unix: i64| -> Result<bool> {
            for (name, _, _) in &changed {
                let full = format!("{}{name}", self.namespace.backup_prefix_at(unix));
                if self.ref_commit(&full)?.is_some() {
                    return Ok(true);
                }
            }
            Ok(false)
        };
        while taken(unix)? {
            unix += 1;
        }
        let backup = self.namespace.backup_prefix_at(unix);
        for (name, old, _) in &changed {
            self.repo
                .reference(
                    format!("{backup}{name}"),
                    *old,
                    PreviousValue::MustNotExist,
                    self.reflog_message(&format!("backup before {}", what.what)),
                )
                .ctx("cannot write the backup ref")?;
        }
        let message = self.reflog_message(&format!("{}, {}", what.what, what.detail));
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
            out.push(BranchRewrite {
                remote,
                old: old.to_string(),
                new: new.to_string(),
                name,
            });
        }
        Ok(HistoryRewrite {
            rewritten,
            branches: out,
            backup,
            tags,
        })
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitApi.blame` (wave 15, strand C): which commit last changed each line of any file.
//!
//! Like `git blame`: the lines of the file at the asked revision are handed down the history,
//! newest commit first. A line the commit did not change (it is in the same place of the
//! parent's version, by the line diff of [`tk_git_types::text_diff`], which the web shell uses
//! too) goes on to the first parent that has it; what no parent has was written by the commit.
//! A file that is missing in a parent but present in the commit may have been renamed: the
//! file the commit removed with the same content, else the most similar one (at least
//! [`SIMILARITY`] of the lines in common), is followed.
//!
//! Paths in and out are repository-relative with a leading `/`; a path without it is relative
//! to the scope.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::Mode;
use tk_git_types::history::{GitBlame, GitBlameHunk};
use tk_git_types::{GitCommit, text_diff};

use crate::commit::describe;
use crate::file_log::{SIMILARITY, similarity};
use crate::repo::Repo;
use crate::util::{Context, Result, Scope, blob_bytes, fail, flatten};

/// Renames are looked for among at most this many removed files, up to this size each.
const RENAME_MAX_FILES: usize = 200;
const RENAME_MAX_BYTES: usize = 512 * 1024;

/// A line of the final file that is not yet blamed: where it is in the commit being looked at.
type Line = (u32, u32);

/// Where each line of `new` is in `old` (`None` for a line the diff added).
fn unchanged(old: &[u8], new: &[u8], new_lines: usize) -> Option<Vec<Option<u32>>> {
    let diff = text_diff("", Some(old), Some(new));
    if diff.binary {
        return None;
    }
    let mut map: Vec<Option<u32>> = vec![None; new_lines];
    let (mut o, mut n) = (0u32, 0u32);
    let same = |o: &mut u32, n: &mut u32, until: u32, map: &mut Vec<Option<u32>>| {
        while *n < until {
            if let Some(slot) = map.get_mut(*n as usize) {
                *slot = Some(*o);
            }
            *o += 1;
            *n += 1;
        }
    };
    for hunk in &diff.hunks {
        // An empty side of a hunk starts at the line before it (unified-diff convention).
        let first_new = if hunk.new_lines == 0 {
            hunk.new_start
        } else {
            hunk.new_start - 1
        };
        same(&mut o, &mut n, first_new, &mut map);
        for line in &hunk.lines {
            match line.kind {
                tk_git_types::DiffLineKind::Context => {
                    if let Some(slot) = map.get_mut(n as usize) {
                        *slot = Some(o);
                    }
                    o += 1;
                    n += 1;
                }
                tk_git_types::DiffLineKind::Added => n += 1,
                tk_git_types::DiffLineKind::Removed => o += 1,
            }
        }
    }
    same(&mut o, &mut n, new_lines as u32, &mut map);
    Some(map)
}

fn line_count(bytes: &[u8]) -> usize {
    bytes.split_inclusive(|b| *b == b'\n').count()
}

/// `Some(text bytes)` for UTF-8 text without NUL, `None` for anything else.
fn text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

impl Repo {
    /// The repository path (without a leading `/`) a history view's path names.
    pub(crate) fn history_path(&self, path: &str) -> Result<BString> {
        match path.strip_prefix('/') {
            Some(rest) => Scope::whole().abs(rest),
            None => self.scope.abs(path),
        }
    }

    /// Who last changed each line of `path` as of `rev`.
    pub fn blame(&self, rev: &str, path: &str) -> Result<GitBlame> {
        let commit = self.resolve_rev(rev)?;
        let repo_path = self.history_path(path)?;
        let shown = format!("/{}", repo_path.to_str_lossy());
        let Some((mode, blob)) = self.entry_at(commit, &repo_path)? else {
            return Err(fail(format!("{shown} does not exist in {rev}")));
        };
        if !matches!(mode, Mode::FILE | Mode::FILE_EXECUTABLE) {
            return Err(fail(format!("{shown} is not a file in {rev}")));
        }
        let bytes = blob_bytes(&self.repo, blob)?;
        let mut out = GitBlame {
            path: shown,
            rev: commit.to_string(),
            lines: 0,
            hunks: Vec::new(),
            binary: false,
        };
        if !text(&bytes) {
            out.binary = true;
            return Ok(out);
        }
        let total = line_count(&bytes);
        out.lines = total as u32;
        if total == 0 {
            return Ok(out);
        }

        // For every final line: (commit, path, 0-based line in that commit's version).
        let mut blamed: Vec<Option<(ObjectId, BString, u32)>> = vec![None; total];
        let mut pending: HashMap<(ObjectId, BString), Vec<Line>> = HashMap::new();
        let mut heap: BinaryHeap<(i64, Reverse<u64>, ObjectId, BString)> = BinaryHeap::new();
        let mut seq = 0u64;
        let mut push = |pending: &mut HashMap<_, Vec<Line>>,
                        heap: &mut BinaryHeap<_>,
                        time: i64,
                        id: ObjectId,
                        path: BString,
                        lines: Vec<Line>| {
            let key = (id, path.clone());
            match pending.get_mut(&key) {
                Some(existing) => existing.extend(lines),
                None => {
                    pending.insert(key, lines);
                    seq += 1;
                    heap.push((time, Reverse(seq), id, path));
                }
            }
        };
        let time_of = |id: ObjectId| -> Result<(i64, Vec<ObjectId>)> {
            let c = self.repo.find_commit(id).ctx("cannot read a commit")?;
            let parents = c.parent_ids().map(|p| p.detach()).collect();
            let time = c
                .decode()
                .ctx("cannot read a commit")?
                .committer()
                .ctx("cannot read a commit")?
                .seconds();
            Ok((time, parents))
        };
        let (t0, _) = time_of(commit)?;
        push(
            &mut pending,
            &mut heap,
            t0,
            commit,
            repo_path,
            (0..total as u32).map(|i| (i, i)).collect(),
        );

        while let Some((_, _, id, path)) = heap.pop() {
            let Some(lines) = pending.remove(&(id, path.clone())) else {
                continue;
            };
            let (_, parents) = time_of(id)?;
            let (_, here) = self
                .entry_at(id, &path)?
                .ok_or_else(|| fail("a file vanished while it was blamed"))?;
            let mut remaining = lines;
            let mut here_bytes: Option<Vec<u8>> = None;
            for parent in parents {
                if remaining.is_empty() {
                    break;
                }
                let mut parent_path = path.clone();
                let mut entry = self.entry_at(parent, &path)?;
                if entry.is_none()
                    && let Some(from) = self.rename_source(parent, id, here, &path)?
                {
                    entry = self.entry_at(parent, &from)?;
                    parent_path = from;
                }
                let Some((pmode, pblob)) = entry else {
                    continue;
                };
                if !matches!(pmode, Mode::FILE | Mode::FILE_EXECUTABLE) {
                    continue;
                }
                let (time, _) = time_of(parent)?;
                if pblob == here {
                    push(
                        &mut pending,
                        &mut heap,
                        time,
                        parent,
                        parent_path,
                        std::mem::take(&mut remaining),
                    );
                    break;
                }
                let new_bytes = match &here_bytes {
                    Some(b) => b,
                    None => here_bytes.insert(blob_bytes(&self.repo, here)?),
                };
                let old_bytes = blob_bytes(&self.repo, pblob)?;
                if !text(&old_bytes) {
                    continue;
                }
                let Some(map) = unchanged(&old_bytes, new_bytes, line_count(new_bytes)) else {
                    continue;
                };
                let (mut pass, mut keep) = (Vec::new(), Vec::new());
                for (fin, cur) in remaining {
                    match map.get(cur as usize).copied().flatten() {
                        Some(old) => pass.push((fin, old)),
                        None => keep.push((fin, cur)),
                    }
                }
                remaining = keep;
                if !pass.is_empty() {
                    push(&mut pending, &mut heap, time, parent, parent_path, pass);
                }
            }
            for (fin, cur) in remaining {
                blamed[fin as usize] = Some((id, path.clone(), cur));
            }
        }

        let mut commits: HashMap<ObjectId, GitCommit> = HashMap::new();
        let mut run: Option<(ObjectId, BString, u32, u32)> = None; // commit, path, orig 0-based, length
        let mut start = 1u32;
        let flush = |run: &mut Option<(ObjectId, BString, u32, u32)>,
                     start: &mut u32,
                     hunks: &mut Vec<GitBlameHunk>,
                     commits: &mut HashMap<ObjectId, GitCommit>|
         -> Result<()> {
            if let Some((id, path, orig, len)) = run.take() {
                let commit = match commits.get(&id) {
                    Some(c) => c.clone(),
                    None => {
                        let c = describe(&self.repo, id)?;
                        commits.insert(id, c.clone());
                        c
                    }
                };
                hunks.push(GitBlameHunk {
                    start: *start,
                    lines: len,
                    commit,
                    path: format!("/{}", path.to_str_lossy()),
                    orig_start: orig + 1,
                });
                *start += len;
            }
            Ok(())
        };
        for b in blamed {
            let (id, path, cur) = b.ok_or_else(|| fail("a line was not blamed"))?;
            match &mut run {
                Some((rid, rpath, orig, len))
                    if *rid == id && *rpath == path && *orig + *len == cur =>
                {
                    *len += 1
                }
                _ => {
                    flush(&mut run, &mut start, &mut out.hunks, &mut commits)?;
                    run = Some((id, path, cur, 1));
                }
            }
        }
        flush(&mut run, &mut start, &mut out.hunks, &mut commits)?;
        Ok(out)
    }

    /// The path in `parent` that `commit` renamed to `path` (content `blob`), if any: any
    /// regular file, not only the scope's files.
    pub(crate) fn rename_source(
        &self,
        parent: ObjectId,
        commit: ObjectId,
        blob: ObjectId,
        path: &BString,
    ) -> Result<Option<BString>> {
        let old = flatten(&self.repo, self.commit_tree(parent)?)?;
        let new = flatten(&self.repo, self.commit_tree(commit)?)?;
        let removed: Vec<(&BString, ObjectId)> = old
            .iter()
            .filter(|(p, b)| {
                matches!(b.mode, Mode::FILE | Mode::FILE_EXECUTABLE)
                    && !new.contains_key(*p)
                    && *p != path
            })
            .map(|(p, b)| (p, b.id))
            .collect();
        if let Some((p, _)) = removed.iter().find(|(_, id)| *id == blob) {
            return Ok(Some((*p).clone()));
        }
        if removed.is_empty() || removed.len() > RENAME_MAX_FILES {
            return Ok(None);
        }
        let mine = blob_bytes(&self.repo, blob)?;
        if mine.len() > RENAME_MAX_BYTES || !text(&mine) {
            return Ok(None);
        }
        let mine = String::from_utf8_lossy(&mine).into_owned();
        let mut best: Option<(f64, &BString)> = None;
        for (p, id) in removed {
            let other = blob_bytes(&self.repo, id)?;
            if other.len() > RENAME_MAX_BYTES || !text(&other) {
                continue;
            }
            let s = similarity(&mine, &String::from_utf8_lossy(&other));
            if s >= SIMILARITY && best.is_none_or(|(b, _)| s > b) {
                best = Some((s, p));
            }
        }
        Ok(best.map(|(_, p)| p.clone()))
    }
}

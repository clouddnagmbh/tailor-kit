// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitApi.searchHistory` (wave 15, strand C): commits by message, author, path and content.
//!
//! The commits come in the graph's order (all refs, children before parents, newest first), so
//! pages are stable while no ref moves. A commit matches when every given field matches:
//!
//! - message, author (name or email): case-insensitive substrings;
//! - path: the commit changed a file whose repository path starts with the given one;
//! - content: the commit added or removed a line that contains the text (case-insensitive),
//!   as `git log -S` finds commits that change how often a string occurs: a line that only
//!   moved does not count;
//! - since, until: the author time.
//!
//! Path and content look at what the commit changed against its first parent (against the
//! empty tree for a root commit). A merge commit changed nothing of its own, so it matches on
//! message, author and time only.
//!
//! A content search reads at most [`HISTORY_SCAN_LIMIT`] commits per call, counted after the
//! `skip` hits that earlier pages showed; `more` tells the caller to ask again with
//! `skip + hits.len()`.

use std::collections::{BTreeMap, HashMap};

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::object::tree::EntryMode;
use rt_app::git::history::{
    GitHistoryField, GitHistoryHit, GitHistoryPage, GitHistoryQuery, HISTORY_SCAN_LIMIT,
};

use crate::repo::Repo;
use crate::util::{Context, Result, blob_bytes};

/// Paths named in a hit at most.
const HIT_PATHS: usize = 5;
/// Files of one commit whose content is looked at, and their size.
const FILES_PER_COMMIT: usize = 200;
const FILE_MAX_BYTES: usize = 2 * 1024 * 1024;

/// A file one commit added, changed or removed (repository path without a leading `/`).
struct Changed {
    path: String,
    old: Option<ObjectId>,
    new: Option<ObjectId>,
}

impl Repo {
    /// Entries of a tree by name.
    fn tree_map(&self, id: ObjectId) -> Result<BTreeMap<BString, (EntryMode, ObjectId)>> {
        let tree = self.repo.find_tree(id).ctx("cannot read a tree")?;
        let mut map = BTreeMap::new();
        for entry in tree.iter() {
            let entry = entry.ctx("cannot read a tree")?;
            map.insert(
                entry.filename().to_owned(),
                (entry.mode(), entry.oid().to_owned()),
            );
        }
        Ok(map)
    }

    /// The files that differ between two trees, below `want` (a path prefix with a leading
    /// `/`) only; unchanged subtrees are skipped by id.
    fn changed_files(
        &self,
        prefix: &str,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
        want: &str,
        out: &mut Vec<Changed>,
    ) -> Result<()> {
        let old_map = match old {
            Some(id) => self.tree_map(id)?,
            None => BTreeMap::new(),
        };
        let new_map = match new {
            Some(id) => self.tree_map(id)?,
            None => BTreeMap::new(),
        };
        let mut names: Vec<&BString> = old_map.keys().chain(new_map.keys()).collect();
        names.sort();
        names.dedup();
        for name in names {
            let (o, n) = (old_map.get(name), new_map.get(name));
            if o == n {
                continue;
            }
            let full = format!("{prefix}{}", name.to_str_lossy());
            let file = |side: Option<&(EntryMode, ObjectId)>| {
                side.filter(|(m, _)| m.is_blob()).map(|(_, id)| *id)
            };
            let tree = |side: Option<&(EntryMode, ObjectId)>| {
                side.filter(|(m, _)| m.is_tree()).map(|(_, id)| *id)
            };
            let (of, nf) = (file(o), file(n));
            if (of.is_some() || nf.is_some()) && format!("/{full}").starts_with(want) {
                out.push(Changed {
                    path: full.clone(),
                    old: of,
                    new: nf,
                });
            }
            let (ot, nt) = (tree(o), tree(n));
            if ot.is_some() || nt.is_some() {
                let dir = format!("/{full}/");
                if dir.starts_with(want) || want.starts_with(&dir) {
                    self.changed_files(&format!("{full}/"), ot, nt, want, out)?;
                }
            }
        }
        Ok(())
    }

    /// Lines of `bytes` that contain `needle` (lower case), by text, with their counts; `None`
    /// for files that are not text.
    fn needle_lines(
        &self,
        id: Option<ObjectId>,
        needle: &str,
    ) -> Result<Option<HashMap<String, i32>>> {
        let mut lines = HashMap::new();
        let Some(id) = id else {
            return Ok(Some(lines));
        };
        let bytes = blob_bytes(&self.repo, id)?;
        if bytes.len() > FILE_MAX_BYTES || bytes.contains(&0) {
            return Ok(None);
        }
        let Ok(text) = String::from_utf8(bytes) else {
            return Ok(None);
        };
        for line in text.lines() {
            let lower = line.to_lowercase();
            if lower.contains(needle) {
                *lines.entry(lower).or_insert(0) += 1;
            }
        }
        Ok(Some(lines))
    }

    /// Per line with `needle` in it: how many more copies the new version has than the old.
    fn needle_delta(&self, change: &Changed, needle: &str) -> Result<HashMap<String, i32>> {
        let mut delta = HashMap::new();
        let (Some(old), Some(new)) = (
            self.needle_lines(change.old, needle)?,
            self.needle_lines(change.new, needle)?,
        ) else {
            return Ok(delta);
        };
        for (line, n) in new {
            *delta.entry(line).or_insert(0) += n;
        }
        for (line, n) in old {
            *delta.entry(line).or_insert(0) -= n;
        }
        delta.retain(|_, n| *n != 0);
        Ok(delta)
    }

    /// The files of a commit in which a line with `needle` was added or removed, empty when
    /// the commit only moved such lines between its files (a rename).
    fn needle_paths(&self, changed: &[Changed], needle: &str) -> Result<Vec<String>> {
        let mut total: HashMap<String, i32> = HashMap::new();
        let mut paths = Vec::new();
        for c in changed.iter().take(FILES_PER_COMMIT) {
            let delta = self.needle_delta(c, needle)?;
            if !delta.is_empty() {
                paths.push(format!("/{}", c.path));
            }
            for (line, n) in delta {
                *total.entry(line).or_insert(0) += n;
            }
        }
        total.retain(|_, n| *n != 0);
        if total.is_empty() {
            return Ok(Vec::new());
        }
        paths.truncate(HIT_PATHS);
        Ok(paths)
    }

    /// One page of the search.
    pub fn search_history(&self, query: &GitHistoryQuery) -> Result<GitHistoryPage> {
        let refs = self.refs()?;
        let tips = self.graph_tips(&refs, query.refs.as_deref())?;
        let order = self.date_order(&tips)?;
        let mut labels = Self::ref_labels(&refs);

        let lower = |s: &Option<String>| {
            s.as_ref()
                .map(|s| s.to_lowercase())
                .filter(|s| !s.is_empty())
        };
        let (message, author, content) = (
            lower(&query.message),
            lower(&query.author),
            lower(&query.content),
        );
        let want = query.path.as_ref().filter(|p| !p.is_empty()).map(|p| {
            if p.starts_with('/') {
                p.clone()
            } else {
                format!("/{p}")
            }
        });
        let limit = query.limit as usize;
        let skip = query.skip as usize;

        let mut page = GitHistoryPage::default();
        let mut seen = 0usize; // matches so far, including the skipped ones
        let mut examined = 0u32; // content searches: commits looked at after the skipped hits
        let mut scanned = 0u32;
        for (id, parents) in &order {
            scanned = scanned.saturating_add(1);
            if content.is_some() && seen >= skip {
                if examined >= HISTORY_SCAN_LIMIT {
                    page.more = true;
                    break;
                }
                examined += 1;
            }
            let commit = self.repo.find_commit(*id).ctx("cannot read a commit")?;
            let decoded = commit.decode().ctx("cannot read a commit")?;
            let who = decoded.author().ctx("cannot read a commit")?;
            let time = who.seconds();
            if query.since.is_some_and(|s| time < s) || query.until.is_some_and(|u| time > u) {
                continue;
            }
            let mut matched = Vec::new();
            if let Some(m) = &message {
                if !decoded.message.to_str_lossy().to_lowercase().contains(m) {
                    continue;
                }
                matched.push(GitHistoryField::Message);
            }
            if let Some(a) = &author {
                let text = format!("{} <{}>", who.name.to_str_lossy(), who.email.to_str_lossy())
                    .to_lowercase();
                if !text.contains(a) {
                    continue;
                }
                matched.push(GitHistoryField::Author);
            }
            let mut paths: Vec<String> = Vec::new();
            if want.is_some() || content.is_some() {
                if parents.len() > 1 {
                    continue;
                }
                let new_tree = Some(self.commit_tree(*id)?);
                let old_tree = match parents.first() {
                    Some(p) => Some(self.commit_tree(*p)?),
                    None => None,
                };
                let mut changed = Vec::new();
                self.changed_files(
                    "",
                    old_tree,
                    new_tree,
                    want.as_deref().unwrap_or("/"),
                    &mut changed,
                )?;
                if changed.is_empty() {
                    continue;
                }
                if want.is_some() {
                    matched.push(GitHistoryField::Path);
                }
                if let Some(needle) = &content {
                    let hit = self.needle_paths(&changed, needle)?;
                    if hit.is_empty() {
                        continue;
                    }
                    matched.push(GitHistoryField::Content);
                    paths = hit;
                } else {
                    paths = changed
                        .iter()
                        .take(HIT_PATHS)
                        .map(|c| format!("/{}", c.path))
                        .collect();
                }
            }
            seen += 1;
            if seen <= skip {
                continue;
            }
            if page.hits.len() >= limit {
                page.more = true;
                break;
            }
            let row = self.graph_commit(*id, parents, &mut labels)?;
            page.hits.push(GitHistoryHit {
                commit: row,
                matched,
                paths,
            });
        }
        page.scanned = scanned;
        Ok(page)
    }
}

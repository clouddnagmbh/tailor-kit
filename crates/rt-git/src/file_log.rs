// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitApi.fileLog`: the commits that changed one file, newest first, following renames and
//! moves like `git log --follow`. A path with a leading `/` names any file of the repository
//! (wave 15: any regular file can be a rename source, and paths come back with the `/`);
//! one without it a collection file, as before.
//!
//! Following: when the followed path exists in a commit but not in its first parent, the
//! commit may have renamed it. Among the collection files (`*.yml`) the commit removed, the
//! one with the same content wins, else the most similar one (at least [`SIMILARITY`] of the
//! lines of the larger file in common; ties go to the smallest path). The entry is then
//! [`GitFileChange::Renamed`] with `from` set, and the older history is followed under the old
//! path. The web shell (`apps/web/src/git/file-log.ts`) uses the same rules, so both give the
//! same log.

use std::collections::{BTreeMap, HashMap};

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::Mode;
use rt_app::git::{GitFileChange, GitFileCommit};

use crate::commit::describe;
use crate::repo::Repo;
use crate::util::{Context, Result, blob_bytes, flatten};

/// Share of lines two versions need in common to count as the same file (renamed with edits).
pub const SIMILARITY: f64 = 0.5;

/// Lines in common over the lines of the larger file, `1.0` for equal files.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let larger = la.len().max(lb.len());
    if larger == 0 {
        return 1.0;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for l in &la {
        *counts.entry(l).or_default() += 1;
    }
    let mut common = 0usize;
    for l in &lb {
        if let Some(n) = counts.get_mut(l)
            && *n > 0
        {
            *n -= 1;
            common += 1;
        }
    }
    common as f64 / larger as f64
}

impl Repo {
    /// Commits of `HEAD`'s history (by commit time) in which `path` (collection-relative)
    /// differs from the commit's first parent, at most `limit`. Follows renames: an entry's
    /// `path` is the file's path at that commit.
    pub fn file_log(&self, path: &str, limit: u32) -> Result<Vec<GitFileCommit>> {
        let Some(head) = self.head_commit()? else {
            return Ok(Vec::new());
        };
        // Validates the path (and is the first followed path). A leading `/` names any file of
        // the repository (wave 15), else a file of the collection.
        self.history_path(path)?;
        let repo_wide = path.starts_with('/');
        let walk = self
            .repo
            .rev_walk([head])
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                Default::default(),
            ))
            .all()
            .ctx("cannot walk the history")?;
        // The path each commit is followed under; parents inherit it.
        let mut followed: HashMap<ObjectId, String> = HashMap::new();
        followed.insert(head, path.to_owned());
        let mut out = Vec::new();
        for info in walk {
            if out.len() >= limit as usize {
                break;
            }
            let info = info.ctx("cannot walk the history")?;
            let here_path = followed
                .get(&info.id)
                .cloned()
                .unwrap_or_else(|| path.to_owned());
            let abs: BString = self.history_path(&here_path)?;
            let here = self.entry_at(info.id, &abs)?.map(|(_, id)| id);
            let parent_id = info.parent_ids.first().copied();
            let parent = match parent_id {
                Some(p) => self.entry_at(p, &abs)?.map(|(_, id)| id),
                None => None,
            };
            let mut parent_path = here_path.clone();
            let change = match (parent, here) {
                (None, Some(blob)) => match parent_id {
                    Some(p) => match if repo_wide {
                        self.rename_source(p, info.id, blob, &abs)?
                            .map(|from| format!("/{}", from.to_str_lossy()))
                    } else {
                        self.renamed_from(p, info.id, blob, &here_path)?
                    } {
                        Some(from) => {
                            parent_path = from.clone();
                            Some((GitFileChange::Renamed, Some(from)))
                        }
                        None => Some((GitFileChange::Added, None)),
                    },
                    None => Some((GitFileChange::Added, None)),
                },
                (Some(_), None) => Some((GitFileChange::Deleted, None)),
                (Some(a), Some(b)) if a != b => Some((GitFileChange::Modified, None)),
                _ => None,
            };
            for (i, p) in info.parent_ids.iter().enumerate() {
                followed.entry(*p).or_insert_with(|| {
                    if i == 0 {
                        parent_path.clone()
                    } else {
                        here_path.clone()
                    }
                });
            }
            if let Some((change, from)) = change {
                out.push(GitFileCommit {
                    commit: describe(&self.repo, info.id)?,
                    path: here_path,
                    change,
                    from,
                });
            }
        }
        Ok(out)
    }

    /// The path in `parent` that `commit` renamed to `path` (content `blob`), if any.
    fn renamed_from(
        &self,
        parent: ObjectId,
        commit: ObjectId,
        blob: ObjectId,
        path: &str,
    ) -> Result<Option<String>> {
        let old = flatten(&self.repo, self.commit_tree(parent)?)?;
        let new = flatten(&self.repo, self.commit_tree(commit)?)?;
        let mut removed: BTreeMap<String, ObjectId> = BTreeMap::new();
        for (p, b) in &old {
            if !matches!(b.mode, Mode::FILE | Mode::FILE_EXECUTABLE) || new.contains_key(p) {
                continue;
            }
            let Some(rel) = self.scope.rel(p.as_bstr()) else {
                continue;
            };
            if rel != path && rt_app::git::tree_path(&rel) {
                removed.insert(rel, b.id);
            }
        }
        if let Some((rel, _)) = removed.iter().find(|(_, id)| **id == blob) {
            return Ok(Some(rel.clone()));
        }
        if removed.is_empty() {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&blob_bytes(&self.repo, blob)?).into_owned();
        let mut best: Option<(f64, String)> = None;
        for (rel, id) in removed {
            let other = String::from_utf8_lossy(&blob_bytes(&self.repo, id)?).into_owned();
            let s = similarity(&text, &other);
            if s >= SIMILARITY && best.as_ref().is_none_or(|(b, _)| s > *b) {
                best = Some((s, rel));
            }
        }
        Ok(best.map(|(_, rel)| rel))
    }
}

impl Repo {
    /// Where the collection sits in the repository: `""` at the root, else `a/b` (no trailing
    /// slash). The key under which recorded results tell two collections of one repository
    /// apart (`rt_app::timeline::TestRecord::collection`).
    pub fn collection_prefix(&self) -> String {
        self.scope.prefix().to_str().unwrap_or_default().to_owned()
    }
}

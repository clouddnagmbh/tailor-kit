// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `status` and `diff`: the working tree of the collection against `HEAD`.

use std::collections::{BTreeMap, BTreeSet};

use gix::bstr::{BStr, BString, ByteSlice, ByteVec};
use gix::index::entry::{Mode, Stage};
use rt_app::git::{GitDiff, GitFile, GitFileState, GitStatus, text_diff};

use crate::repo::Repo;
use crate::util::{Context, Result, blob_bytes, fail, flatten};
use crate::worktree::{FileState, Probe, fs_path, same};

impl Repo {
    /// Branch, upstream, ahead/behind and the changed files of the collection.
    pub fn status(&self) -> Result<GitStatus> {
        let head = flatten(&self.repo, self.head_tree()?)?;
        let index = self.load_index()?;
        let scope = &self.scope;

        let mut conflicted = BTreeSet::new();
        let mut tracked = BTreeSet::new();
        for entry in index.entries() {
            let path = entry.path(&index);
            if !scope.contains(path) {
                continue;
            }
            if entry.stage() == Stage::Unconflicted {
                if entry.mode != Mode::COMMIT {
                    tracked.insert(path.to_owned());
                }
            } else {
                conflicted.insert(path.to_owned());
            }
        }
        for (path, blob) in &head {
            if blob.mode != Mode::COMMIT && scope.contains(path.as_bstr()) {
                tracked.insert(path.clone());
            }
        }

        let mut files = BTreeMap::new();
        let mut probe = Probe::new(self)?;
        for path in tracked.difference(&conflicted) {
            let state = match (head.get(path), probe.state(path.as_bstr(), &index)?) {
                (Some(h), FileState::File(b)) if same(*h, b) => continue,
                (_, FileState::File(_)) => GitFileState::Modified,
                (Some(_), _) => GitFileState::Deleted,
                // Added to the index by someone else and deleted again: nothing to commit.
                (None, _) => continue,
            };
            files.insert(path.clone(), state);
        }
        for path in &conflicted {
            files.insert(path.clone(), GitFileState::Conflicted);
        }
        let mut known = tracked;
        known.extend(conflicted);
        for path in self.untracked(&index, &known)? {
            files.insert(path, GitFileState::Untracked);
        }

        let (upstream, ahead, behind) = match self.upstream()? {
            Some(up) => {
                let (ahead, behind) = self.ahead_behind(&up.tracking)?;
                (Some(up.display), ahead, behind)
            }
            None => (None, 0, 0),
        };
        let branch = match self.head_commit()? {
            Some(_) => self.head_branch()?,
            None => None,
        };
        Ok(GitStatus {
            repo: true,
            branch,
            upstream,
            ahead,
            behind,
            files: files
                .into_iter()
                .filter_map(|(path, state)| {
                    scope
                        .rel(path.as_bstr())
                        .map(|path| GitFile { path, state })
                })
                .collect(),
            merging: self.merge_head()?.is_some(),
        })
    }

    /// Files of the collection that are neither tracked nor ignored (repository paths).
    fn untracked(
        &self,
        index: &gix::index::State,
        known: &BTreeSet<BString>,
    ) -> Result<Vec<BString>> {
        let mut excludes = self
            .repo
            .excludes(
                index,
                None,
                gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
            )
            .ctx("cannot read .gitignore")?;
        let tracked_below = |dir: &BStr| {
            let mut start = dir.to_owned();
            start.push(b'/');
            known
                .range(start.clone()..)
                .next()
                .is_some_and(|p| p.starts_with(start.as_slice()))
        };
        let mut out = Vec::new();
        let mut stack = vec![(self.root.clone(), self.scope.prefix().to_owned(), false)];
        while let Some((dir, rel, parent_excluded)) = stack.pop() {
            let entries = std::fs::read_dir(&dir).ctx("cannot read a folder")?;
            for entry in entries {
                let entry = entry.ctx("cannot read a folder")?;
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if name == ".git" {
                    continue;
                }
                let mut path = rel.clone();
                if !path.is_empty() {
                    path.push(b'/');
                }
                path.push_str(name.as_bytes());
                let kind = entry.file_type().ctx("cannot read a folder")?;
                if kind.is_dir() {
                    if entry.path().join(".git").exists() {
                        // A nested repository: not ours to list.
                        continue;
                    }
                    let excluded = parent_excluded
                        || excludes
                            .at_entry(path.as_bstr(), Some(Mode::DIR))
                            .ctx("cannot read .gitignore")?
                            .is_excluded();
                    if excluded && !tracked_below(path.as_bstr()) {
                        continue;
                    }
                    stack.push((entry.path(), path, excluded));
                } else if !known.contains(&path) {
                    let excluded = parent_excluded
                        || excludes
                            .at_entry(path.as_bstr(), Some(Mode::FILE))
                            .ctx("cannot read .gitignore")?
                            .is_excluded();
                    if !excluded {
                        out.push(path);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Commits on HEAD that are not on `tracking`, and the other way round.
    pub(crate) fn ahead_behind(&self, tracking: &str) -> Result<(u32, u32)> {
        let Some(head) = self.head_commit()? else {
            return Ok((0, 0));
        };
        let up = self.ref_commit(tracking)?;
        let count = |tip: gix::ObjectId, hidden: Option<gix::ObjectId>| -> Result<u32> {
            let walk = self
                .repo
                .rev_walk([tip])
                .with_hidden(hidden)
                .all()
                .ctx("cannot walk the history")?;
            let mut n = 0u32;
            for info in walk {
                info.ctx("cannot walk the history")?;
                n += 1;
            }
            Ok(n)
        };
        let ahead = count(head, up)?;
        let behind = match up {
            Some(up) => count(up, Some(head))?,
            None => 0,
        };
        Ok((ahead, behind))
    }

    /// Line diff of one file of the collection, `HEAD` → working tree.
    pub fn diff(&self, path: &str) -> Result<GitDiff> {
        let repo_path = self.scope.abs(path)?;
        let head = flatten(&self.repo, self.head_tree()?)?;
        let old = match head.get(&repo_path) {
            Some(blob) if blob.mode != Mode::COMMIT => Some(blob_bytes(&self.repo, blob.id)?),
            _ => None,
        };
        let file = fs_path(&self.workdir, repo_path.as_bstr())?;
        let new = match std::fs::symlink_metadata(&file) {
            Ok(md) if md.is_symlink() => {
                let target = std::fs::read_link(&file).ctx("cannot read a symbolic link")?;
                Some(
                    gix::path::into_bstr(target)
                        .ctx("invalid symbolic link")?
                        .into_owned()
                        .into(),
                )
            }
            Ok(md) if md.is_file() => Some(std::fs::read(&file).ctx("cannot read a file")?),
            Ok(_) => None,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(fail(format!("cannot read {path}: {e}"))),
        };
        Ok(text_diff(path, old.as_deref(), new.as_deref()))
    }
}

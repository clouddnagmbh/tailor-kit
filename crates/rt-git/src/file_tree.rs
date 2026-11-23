// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The entries of one folder (`GitApi.fileTree`, wave 15 strand B): at a revision (the tree of
//! the commit) or in the working tree, where every entry also says whether it changed and
//! whether it is ignored.

use std::collections::BTreeMap;

use gix::bstr::{BStr, BString, ByteSlice, ByteVec};
use gix::index::entry::Mode;
use rt_app::git::GitFileState;
use rt_app::git::files::{GitEntryKind, GitTreeEntry};

use crate::blob::repo_path;
use crate::repo::Repo;
use crate::util::{Context, Result, fail};
use crate::worktree::{FileState, Probe, same};

fn kind_of_mode(mode: Mode) -> GitEntryKind {
    if mode == Mode::COMMIT {
        GitEntryKind::Submodule
    } else if mode == Mode::SYMLINK {
        GitEntryKind::Symlink
    } else if mode == Mode::DIR {
        GitEntryKind::Dir
    } else {
        GitEntryKind::File
    }
}

fn join(dir: &BStr, name: &str) -> BString {
    let mut path = dir.to_owned();
    if !path.is_empty() {
        path.push(b'/');
    }
    path.push_str(name.as_bytes());
    path
}

fn api(path: &BStr) -> String {
    format!("/{}", path.to_str_lossy())
}

/// Folders first, then by name (case-insensitive, then exact).
fn sort(entries: &mut [GitTreeEntry]) {
    entries.sort_by(|a, b| {
        let dir = |e: &GitTreeEntry| e.kind != GitEntryKind::Dir;
        dir(a)
            .cmp(&dir(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
}

impl Repo {
    /// The entries of the folder `dir` (repository-relative; `""` or `/` is the root).
    pub fn file_tree(&self, rev: Option<&str>, dir: &str) -> Result<Vec<GitTreeEntry>> {
        let dir = dir.trim_matches('/');
        let dir_path = if dir.is_empty() {
            BString::default()
        } else {
            repo_path(dir)?
        };
        match rev {
            Some(rev) => self.tree_entries(rev, dir_path.as_bstr()),
            None => self.worktree_entries(dir_path.as_bstr()),
        }
    }

    /// The entries of a folder in the tree of the commit `rev`: `(mode, id)` by name.
    fn folder_at(
        &self,
        commit: Option<gix::ObjectId>,
        dir: &BStr,
    ) -> Result<BTreeMap<String, (Mode, gix::ObjectId)>> {
        let Some(commit) = commit else {
            return Ok(BTreeMap::new());
        };
        let root = self
            .repo
            .find_tree(self.commit_tree(commit)?)
            .ctx("cannot read a tree")?;
        let tree = if dir.is_empty() {
            root
        } else {
            let path = dir
                .to_str()
                .map_err(|_| fail("the path is not valid UTF-8"))?;
            match root
                .lookup_entry_by_path(std::path::Path::new(path))
                .ctx("cannot read a tree")?
            {
                Some(e) if e.mode().is_tree() => self
                    .repo
                    .find_tree(e.object_id())
                    .ctx("cannot read a tree")?,
                _ => return Ok(BTreeMap::new()),
            }
        };
        let mut out = BTreeMap::new();
        for entry in tree.iter() {
            let entry = entry.ctx("cannot read a tree")?;
            if let Ok(name) = entry.filename().to_str() {
                out.insert(
                    name.to_owned(),
                    (Mode::from(entry.mode()), entry.oid().to_owned()),
                );
            }
        }
        Ok(out)
    }

    fn tree_entries(&self, rev: &str, dir: &BStr) -> Result<Vec<GitTreeEntry>> {
        let commit = self.resolve_rev(rev)?;
        let folder = self.folder_at(Some(commit), dir)?;
        let mut out = Vec::with_capacity(folder.len());
        for (name, (mode, id)) in folder {
            let kind = kind_of_mode(mode);
            let size = if kind == GitEntryKind::File || kind == GitEntryKind::Symlink {
                self.repo.find_header(id).ok().map(|h| h.size())
            } else {
                None
            };
            out.push(GitTreeEntry {
                path: api(join(dir, &name).as_bstr()),
                name,
                kind,
                size,
                state: None,
                ignored: false,
            });
        }
        sort(&mut out);
        Ok(out)
    }

    fn worktree_entries(&self, dir: &BStr) -> Result<Vec<GitTreeEntry>> {
        let folder = crate::worktree::fs_path(&self.workdir, dir)?;
        let head = self.folder_at(self.head_commit()?, dir)?;
        let index = self.load_index()?;
        let mut excludes = self
            .repo
            .excludes(
                &index,
                None,
                gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
            )
            .ctx("cannot read .gitignore")?;
        let mut probe = Probe::new(self)?;
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let entries = std::fs::read_dir(&folder)
            .map_err(|e| fail(format!("cannot read the folder {}: {e}", api(dir))))?;
        for entry in entries {
            let entry = entry.ctx("cannot read a folder")?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name == ".git" {
                continue;
            }
            let path = join(dir, &name);
            let file_type = entry.file_type().ctx("cannot read a folder")?;
            let md = entry.metadata().ok();
            seen.insert(name.clone());
            if file_type.is_dir() {
                let mut prefix = path.clone();
                prefix.push(b'/');
                let tracked = index
                    .entries()
                    .iter()
                    .any(|e| e.path(&index).starts_with(prefix.as_slice()))
                    || head.contains_key(&name);
                let submodule = head.get(&name).is_some_and(|(m, _)| *m == Mode::COMMIT)
                    || entry.path().join(".git").exists();
                let ignored = !tracked
                    && !submodule
                    && excludes
                        .at_entry(path.as_bstr(), Some(Mode::DIR))
                        .ctx("cannot read .gitignore")?
                        .is_excluded();
                out.push(GitTreeEntry {
                    path: api(path.as_bstr()),
                    name,
                    kind: if submodule {
                        GitEntryKind::Submodule
                    } else {
                        GitEntryKind::Dir
                    },
                    size: None,
                    state: None,
                    ignored,
                });
                continue;
            }
            let kind = if file_type.is_symlink() {
                GitEntryKind::Symlink
            } else {
                GitEntryKind::File
            };
            let tracked = index
                .entry_by_path_and_stage(path.as_bstr(), gix::index::entry::Stage::Unconflicted)
                .is_some()
                || head.contains_key(&name);
            let (state, ignored) = if tracked {
                let now = probe.state(path.as_bstr(), &index)?;
                let state = match (head.get(&name), now) {
                    (Some((m, id)), FileState::File(w)) => {
                        let h = crate::util::Blob { mode: *m, id: *id };
                        (!same(h, w)).then_some(GitFileState::Modified)
                    }
                    (Some(_), _) => Some(GitFileState::Deleted),
                    (None, FileState::File(_)) => Some(GitFileState::Untracked),
                    (None, _) => None,
                };
                (state, false)
            } else if excludes
                .at_entry(path.as_bstr(), Some(Mode::FILE))
                .ctx("cannot read .gitignore")?
                .is_excluded()
            {
                (None, true)
            } else {
                (Some(GitFileState::Untracked), false)
            };
            out.push(GitTreeEntry {
                path: api(path.as_bstr()),
                name,
                kind,
                size: md.map(|m| m.len()),
                state,
                ignored,
            });
        }
        // Files `HEAD` has that the working tree lost.
        for (name, (mode, _)) in &head {
            if seen.contains(name) || *mode == Mode::DIR {
                continue;
            }
            out.push(GitTreeEntry {
                path: api(join(dir, name).as_bstr()),
                name: name.clone(),
                kind: kind_of_mode(*mode),
                size: None,
                state: (*mode != Mode::COMMIT).then_some(GitFileState::Deleted),
                ignored: false,
            });
        }
        sort(&mut out);
        Ok(out)
    }
}

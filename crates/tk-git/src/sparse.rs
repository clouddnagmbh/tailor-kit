// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Skip-worktree entries (a sparse checkout, GitTailor wave 3): Git keeps them in the index with
//! the flag `skip-worktree` and treats them as unchanged, whatever the working tree holds at
//! their path (usually nothing). The kit does the same everywhere it reads the working tree
//! ([`assumed`], used by the probe), keeps the flag when it rebuilds the index ([`keep`]), and
//! never writes or deletes such a file when it moves the working tree. Which paths are sparse
//! (the patterns of `.git/info/sparse-checkout`) is the product's business: it sets the flags
//! with [`Repo::set_skip_worktree`]. Without skip-worktree entries nothing changes.

use std::collections::BTreeSet;

use gix::bstr::{BStr, BString, ByteSlice};
use gix::index::entry::{Flags, Mode, Stage};

use crate::repo::Repo;
use crate::util::{Blob, Result};
use crate::worktree::{FileState, Probe, blocked_error, same, stat_of};

/// The entry of `path` (stage 0) when it is skip-worktree: what Git assumes the working tree
/// holds there.
pub(crate) fn assumed(path: &BStr, index: &gix::index::State) -> Option<Blob> {
    let e = index.entry_by_path_and_stage(path, Stage::Unconflicted)?;
    e.flags.contains(Flags::SKIP_WORKTREE).then_some(Blob {
        mode: e.mode,
        id: e.id,
    })
}

/// The skip-worktree paths of `index` (stage 0).
pub(crate) fn skipped(index: &gix::index::State) -> BTreeSet<BString> {
    index
        .entries()
        .iter()
        .filter(|e| e.stage() == Stage::Unconflicted && e.flags.contains(Flags::SKIP_WORKTREE))
        .map(|e| e.path(index).to_owned())
        .collect()
}

fn set_flag(flags: &mut Flags, on: bool) {
    if on {
        flags.insert(Flags::SKIP_WORKTREE | Flags::EXTENDED);
    } else {
        flags.remove(Flags::SKIP_WORKTREE);
        if !flags.contains(Flags::INTENT_TO_ADD) {
            flags.remove(Flags::EXTENDED);
        }
    }
}

/// Marks the stage-0 entries of `index` at `paths` skip-worktree again (after the index was
/// rebuilt from a tree, which knows no flags).
pub(crate) fn keep(index: &mut gix::index::State, paths: &BTreeSet<BString>) {
    if paths.is_empty() {
        return;
    }
    let (entries, backing) = index.entries_mut_and_pathbacking();
    for e in entries {
        if e.stage() == Stage::Unconflicted && paths.contains(e.path_in(backing)) {
            set_flag(&mut e.flags, true);
        }
    }
}

/// What [`Repo::set_skip_worktree`] changed (repository paths, leading `/`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkipChange {
    /// Now skip-worktree; their files were removed from the working tree.
    pub hidden: Vec<String>,
    /// No longer skip-worktree; their files were written.
    pub shown: Vec<String>,
}

impl Repo {
    /// Sets the skip-worktree flag of every tracked file (stage 0, no submodules) to
    /// `skip(path)` (repository path without leading `/`) and makes the working tree follow,
    /// like `git sparse-checkout reapply`: a newly skipped file is removed, a file no longer
    /// skipped is written from the index. Refuses with nothing changed when a file to remove has
    /// local changes, or when an untracked file (or folder) differs where one is to be written.
    /// Conflicted paths are never skipped.
    pub fn set_skip_worktree(&self, skip: &dyn Fn(&str) -> bool) -> Result<SkipChange> {
        let whole = self.whole()?;
        let mut index = whole.load_index()?;
        let conflicted: BTreeSet<BString> = index
            .entries()
            .iter()
            .filter(|e| e.stage() != Stage::Unconflicted)
            .map(|e| e.path(&index).to_owned())
            .collect();
        let mut hide: Vec<(BString, Blob)> = Vec::new();
        let mut show: Vec<(BString, Blob)> = Vec::new();
        for e in index.entries() {
            if e.stage() != Stage::Unconflicted || e.mode == Mode::COMMIT {
                continue;
            }
            let path = e.path(&index);
            if conflicted.contains(path) {
                continue;
            }
            let want = skip(&path.to_str_lossy());
            let is = e.flags.contains(Flags::SKIP_WORKTREE);
            let blob = Blob {
                mode: e.mode,
                id: e.id,
            };
            if want && !is {
                hide.push((path.to_owned(), blob));
            } else if !want && is {
                show.push((path.to_owned(), blob));
            }
        }
        // Check everything before the first change.
        let mut probe = Probe::new(&whole)?;
        let mut stuck = Vec::new();
        for (p, blob) in &hide {
            match probe.state(p.as_bstr(), &index)? {
                FileState::File(b) if same(*blob, b) => {}
                FileState::Missing => {}
                _ => stuck.push(p.clone()),
            }
        }
        // While the flag is set the probe assumes the index; look at the disk itself.
        let plain = {
            let mut copy = index.clone();
            for e in copy.entries_mut() {
                set_flag(&mut e.flags, false);
            }
            copy
        };
        for (p, blob) in &show {
            match probe.state(p.as_bstr(), &plain)? {
                FileState::Missing => {}
                FileState::File(b) if same(*blob, b) => {}
                _ => stuck.push(p.clone()),
            }
        }
        if !stuck.is_empty() {
            stuck.sort();
            return Err(blocked_error(&whole, "change the sparse checkout", &stuck));
        }
        let mut change = SkipChange::default();
        for (p, _) in &hide {
            crate::worktree::remove(&whole.workdir, p.as_bstr())?;
            change.hidden.push(format!("/{}", p.to_str_lossy()));
        }
        drop(probe);
        // Written like a checkout: the content filter smudges them, prepared in one batch.
        let mut writer = Probe::for_checkout(&whole, &index)?;
        let wanted: Vec<(&BStr, Blob)> = show.iter().map(|(p, b)| (p.as_bstr(), *b)).collect();
        writer.prepare(&wanted)?;
        let mut written = std::collections::BTreeMap::new();
        for (p, blob) in &show {
            let file = writer.write(p.as_bstr(), *blob)?;
            written.insert(p.clone(), stat_of(&file));
            change.shown.push(format!("/{}", p.to_str_lossy()));
        }
        if change.hidden.is_empty() && change.shown.is_empty() {
            return Ok(change);
        }
        let hidden: BTreeSet<&BString> = hide.iter().map(|(p, _)| p).collect();
        let (entries, backing) = index.entries_mut_and_pathbacking();
        for e in entries {
            if e.stage() != Stage::Unconflicted {
                continue;
            }
            let path = e.path_in(backing);
            if hidden.contains(&path.to_owned()) {
                set_flag(&mut e.flags, true);
            } else if let Some(stat) = written.get(path) {
                set_flag(&mut e.flags, false);
                e.stat = *stat;
            }
        }
        whole.write_index(&mut index)?;
        Ok(change)
    }
}

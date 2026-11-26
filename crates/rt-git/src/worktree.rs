// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The working tree: what a file holds right now ([`Probe`]), and moving the whole working
//! tree from one tree to another without losing local work ([`switch`]).

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use gix::bstr::{BStr, BString, ByteSlice};
use gix::filter::plumbing::driver::apply::{Delay, MaybeDelayed};
use gix::filter::plumbing::pipeline::convert::{ToGitOutcome, ToWorktreeOutcome, to_worktree};
use gix::index::entry::{Mode, Stage, Stat};

use crate::repo::Repo;
use crate::util::{Blob, Context, Flat, Result, blob_bytes, fail, flatten};

/// A path in the working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Missing,
    /// A folder where a file could be.
    Dir,
    File(Blob),
}

/// Equal content and kind. Without reliable executable bits (Windows) the modes of files
/// are not compared.
pub fn same(a: Blob, b: Blob) -> bool {
    if a.id != b.id {
        return false;
    }
    if cfg!(unix) {
        a.mode == b.mode
    } else {
        let file = |m: Mode| m == Mode::FILE || m == Mode::FILE_EXECUTABLE;
        a.mode == b.mode || (file(a.mode) && file(b.mode))
    }
}

/// Stat comparison that works on every platform: whole-second mtime and size.
fn stat_options() -> gix::index::entry::stat::Options {
    gix::index::entry::stat::Options {
        trust_ctime: false,
        check_stat: false,
        use_nsec: false,
        use_stdev: false,
    }
}

pub fn fs_path(workdir: &Path, path: &BStr) -> Result<PathBuf> {
    let rel = gix::path::from_bstr(path).map_err(|_| fail(format!("invalid path {path}")))?;
    Ok(workdir.join(rel))
}

/// Stat of a file just written, for the index.
pub fn stat_of(path: &Path) -> Stat {
    gix::index::fs::Metadata::from_path_no_follow(path)
        .ok()
        .and_then(|md| Stat::from_fs(&md).ok())
        .unwrap_or_default()
}

/// Reads working-tree files the way Git sees them (after clean filters such as `eol`).
pub struct Probe<'r> {
    repo: &'r gix::Repository,
    workdir: &'r Path,
    pipeline: gix::filter::Pipeline<'r>,
}

impl<'r> Probe<'r> {
    pub fn new(repo: &'r Repo) -> Result<Self> {
        let (pipeline, _) = repo
            .repo
            .filter_pipeline(None)
            .ctx("cannot read the attributes")?;
        Ok(Self {
            repo: &repo.repo,
            workdir: &repo.workdir,
            pipeline,
        })
    }

    /// What the working tree holds at `path`. `index` lets unchanged files skip hashing.
    pub fn state(&mut self, path: &BStr, index: &gix::index::State) -> Result<FileState> {
        let file = fs_path(self.workdir, path)?;
        let md = match std::fs::symlink_metadata(&file) {
            Ok(md) => md,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(FileState::Missing);
            }
            Err(e) => return Err(fail(format!("cannot read {path}: {e}"))),
        };
        let hash = self.repo.object_hash();
        if md.is_dir() {
            return Ok(FileState::Dir);
        }
        if md.is_symlink() {
            let target = std::fs::read_link(&file).ctx("cannot read a symbolic link")?;
            let bytes = gix::path::into_bstr(target)
                .ctx("invalid symbolic link")?
                .into_owned();
            let id = gix::objs::compute_hash(hash, gix::objs::Kind::Blob, &bytes)
                .ctx("cannot hash a file")?;
            return Ok(FileState::File(Blob {
                mode: Mode::SYMLINK,
                id,
            }));
        }
        let mode = if gix::fs::is_executable(&md) {
            Mode::FILE_EXECUTABLE
        } else {
            Mode::FILE
        };
        if let Some(entry) = index.entry_by_path_and_stage(path, Stage::Unconflicted)
            && (entry.mode == Mode::FILE || entry.mode == Mode::FILE_EXECUTABLE)
        {
            let stat = stat_of(&file);
            let opts = stat_options();
            if entry.stat.matches(&stat, opts) && !entry.stat.is_racy(index.timestamp(), opts) {
                return Ok(FileState::File(Blob { mode, id: entry.id }));
            }
        }
        let bytes = std::fs::read(&file).map_err(|e| fail(format!("cannot read {path}: {e}")))?;
        let rel = gix::path::from_bstr(path).map_err(|_| fail(format!("invalid path {path}")))?;
        let data = match self
            .pipeline
            .convert_to_git(&bytes[..], &rel, index)
            .ctx("cannot convert a file")?
        {
            ToGitOutcome::Unchanged(_) => bytes,
            ToGitOutcome::Buffer(buf) => buf.to_vec(),
            ToGitOutcome::Process(mut read) => {
                let mut out = Vec::new();
                read.read_to_end(&mut out).ctx("cannot convert a file")?;
                out
            }
        };
        let id = gix::objs::compute_hash(hash, gix::objs::Kind::Blob, &data)
            .ctx("cannot hash a file")?;
        Ok(FileState::File(Blob { mode, id }))
    }

    /// Writes `blob` to `path` (smudge filters applied), replacing whatever file is there.
    pub fn write(&mut self, path: &BStr, blob: Blob) -> Result<PathBuf> {
        let file = fs_path(self.workdir, path)?;
        let io = |e: std::io::Error| fail(format!("cannot write {path}: {e}"));
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        if let Ok(md) = std::fs::symlink_metadata(&file)
            && !md.is_dir()
        {
            std::fs::remove_file(&file).map_err(io)?;
        }
        let data = blob_bytes(self.repo, blob.id)?;
        if blob.mode == Mode::SYMLINK {
            write_symlink(&data, &file).map_err(io)?;
            return Ok(file);
        }
        let out = match self
            .pipeline
            .convert_to_worktree(
                &data,
                path,
                to_worktree::Options {
                    can_delay: Delay::Forbid,
                    unknown_encoding: Default::default(),
                },
            )
            .ctx("cannot convert a file")?
        {
            ToWorktreeOutcome::Unchanged(b) => b.to_vec(),
            ToWorktreeOutcome::Buffer(b) => b.to_vec(),
            ToWorktreeOutcome::Process(MaybeDelayed::Immediate(mut read)) => {
                let mut out = Vec::new();
                read.read_to_end(&mut out).ctx("cannot convert a file")?;
                out
            }
            ToWorktreeOutcome::Process(MaybeDelayed::Delayed(_)) => {
                return Err(fail(format!("a filter delayed {path}")));
            }
        };
        std::fs::write(&file, out).map_err(io)?;
        set_executable(&file, blob.mode == Mode::FILE_EXECUTABLE).map_err(io)?;
        Ok(file)
    }
}

#[cfg(unix)]
fn write_symlink(target: &[u8], at: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target), at)
}

#[cfg(not(unix))]
fn write_symlink(target: &[u8], at: &Path) -> std::io::Result<()> {
    // Like Git with `core.symlinks=false`: a plain file holding the link target.
    std::fs::write(at, target)
}

#[cfg(unix)]
fn set_executable(file: &Path, exec: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perm = std::fs::metadata(file)?.permissions();
    let mode = perm.mode();
    let next = if exec {
        mode | ((mode & 0o444) >> 2)
    } else {
        mode & !0o111
    };
    if next != mode {
        perm.set_mode(next);
        std::fs::set_permissions(file, perm)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_file: &Path, _exec: bool) -> std::io::Result<()> {
    Ok(())
}

/// Removes `path` and the folders above it that became empty (up to the work tree root).
fn remove(workdir: &Path, path: &BStr) -> Result<()> {
    let file = fs_path(workdir, path)?;
    match std::fs::remove_file(&file) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(fail(format!("cannot delete {path}: {e}"))),
    }
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == workdir || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
    Ok(())
}

/// Paths whose entry differs between `from` and `to` (submodules left alone).
fn changed(from: &Flat, to: &Flat) -> BTreeSet<BString> {
    from.keys()
        .chain(to.keys())
        .filter(|p| from.get(*p) != to.get(*p))
        .filter(|p| {
            let sub = |f: &Flat| f.get(*p).is_some_and(|b| b.mode == Mode::COMMIT);
            !(sub(from) || sub(to))
        })
        .cloned()
        .collect()
}

/// Paths `switch` from `from` to `to` would refuse to touch: tracked files with local
/// changes, untracked files (or folders) in the way. Repository paths.
pub fn blocked(
    repo: &Repo,
    from: &Flat,
    to: &Flat,
    index: &gix::index::State,
) -> Result<Vec<BString>> {
    let mut probe = Probe::new(repo)?;
    let mut blocked = Vec::new();
    let paths = changed(from, to);
    for p in &paths {
        // Fine: the file is as committed in `from`, or already as it will be in `to`.
        let ok = match probe.state(p.as_bstr(), index)? {
            FileState::File(b) => {
                from.get(p).is_some_and(|f| same(*f, b)) || to.get(p).is_some_and(|t| same(*t, b))
            }
            FileState::Missing => !from.contains_key(p) || !to.contains_key(p),
            FileState::Dir => false,
        };
        if !ok {
            blocked.push(p.clone());
            continue;
        }
        // A file that is about to be created must not sit below an untracked file.
        if to.contains_key(p) {
            let mut parent = p.as_bstr();
            while let Some(pos) = parent.rfind_byte(b'/') {
                parent = parent[..pos].as_bstr();
                let removed = from.contains_key(parent) && !to.contains_key(parent);
                if !removed && let FileState::File(_) = probe.state(parent, index)? {
                    blocked.push(parent.to_owned());
                }
            }
        }
    }
    blocked.sort();
    blocked.dedup();
    Ok(blocked)
}

/// Error for [`blocked`] paths (shown relative to the collection where possible).
pub fn blocked_error(repo: &Repo, what: &str, paths: &[BString]) -> crate::util::GitError {
    let mut names: Vec<String> = paths
        .iter()
        .take(5)
        .map(|p| repo.scope.rel(p.as_bstr()).unwrap_or_else(|| p.to_string()))
        .collect();
    if paths.len() > 5 {
        names.push(format!("and {} more", paths.len() - 5));
    }
    fail(format!(
        "cannot {what}: local changes would be overwritten ({}). Commit them first.",
        names.join(", ")
    ))
}

/// Moves the working tree from tree `from` to tree `to`: deletes and writes exactly the files
/// that differ, after checking that none of them has local changes. Returns the index for
/// `to` (not written yet: the caller may add conflict stages first).
pub fn switch(
    repo: &Repo,
    from: gix::ObjectId,
    to: gix::ObjectId,
    what: &str,
) -> Result<gix::index::File> {
    let from = flatten(&repo.repo, from)?;
    let to_flat = flatten(&repo.repo, to)?;
    let old = repo.load_index()?;
    let blocked_paths = blocked(repo, &from, &to_flat, &old)?;
    if !blocked_paths.is_empty() {
        return Err(blocked_error(repo, what, &blocked_paths));
    }
    let paths = changed(&from, &to_flat);
    for p in &paths {
        if !to_flat.contains_key(p) {
            remove(&repo.workdir, p.as_bstr())?;
        }
    }
    let mut probe = Probe::new(repo)?;
    let mut written = std::collections::BTreeMap::new();
    for p in &paths {
        if let Some(blob) = to_flat.get(p) {
            let file = probe.write(p.as_bstr(), *blob)?;
            written.insert(p.clone(), stat_of(&file));
        }
    }
    let mut index = repo
        .repo
        .index_from_tree(&to)
        .ctx("cannot build the index")?;
    let (entries, backing) = index.entries_mut_and_pathbacking();
    for entry in entries {
        let path = entry.path_in(backing);
        if let Some(stat) = written.get(path) {
            entry.stat = *stat;
        } else if let Some(prev) = old.entry_by_path_and_stage(path, Stage::Unconflicted)
            && prev.id == entry.id
            && prev.mode == entry.mode
        {
            entry.stat = prev.stat;
        }
    }
    Ok(index)
}

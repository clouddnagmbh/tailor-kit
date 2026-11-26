// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The working copy of the whole repository (`GitApi.workingCopy`, wave 15 strand B).
//!
//! Unlike [`Repo::status`], which lists the files of one scope, this looks at the whole
//! work tree: paths are repository-relative with a leading `/`, nothing is scoped. Ignored
//! files never appear. Every changed file says how the working tree differs from `HEAD`
//! (what the default "commit without staging" form commits) and, separately, what the index
//! holds ([`GitWorkingFile::staged`], [`GitWorkingFile::unstaged`]) for a staging view.
//!
//! Budget `git_status_large`: 50 000 tracked files, 100 of them changed, under 300 ms. The
//! cost is `lstat` for every tracked file, so that runs on several threads; files whose size
//! and modification time match the index are never read.

use std::collections::{HashMap, HashSet};

use gix::bstr::{BStr, BString, ByteSlice, ByteVec};
use gix::index::entry::{Mode, Stage, Stat};
use tk_git_types::files::{
    BLOB_TEXT_LIMIT, GitEntryKind, GitWorkingCopy, GitWorkingFile, WORKING_COPY_LIMIT,
};
use tk_git_types::repos::GitOperation;
use tk_git_types::{GitFileChange, GitFileState};

use crate::repo::{Repo, Underway};
use crate::util::{Blob, Context, Result, fail, flatten};
use crate::worktree::{FileState, Probe, fs_path, same};

/// More new files than this are not hashed to look for renames.
const RENAME_CANDIDATES: usize = 2_000;

/// Tracked files per thread, at least.
const CHUNK: usize = 4_096;

fn stat_options() -> gix::index::entry::stat::Options {
    gix::index::entry::stat::Options {
        trust_ctime: false,
        check_stat: false,
        use_nsec: false,
        use_stdev: false,
    }
}

/// A tracked file as the index records it.
struct Item<'a> {
    path: &'a BStr,
    mode: Mode,
    id: gix::ObjectId,
    stat: Stat,
    /// Modified in the same second as the index was written: size and time prove nothing.
    racy: bool,
}

/// What a tracked path looks like on disk, from `lstat` alone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// Size and modification time match the index and the entry is not racy.
    Clean,
    Missing,
    /// Something to read and hash.
    Dirty,
}

fn look(workdir: &std::path::Path, item: &Item<'_>) -> Look {
    let Ok(file) = fs_path(workdir, item.path) else {
        return Look::Dirty;
    };
    let md = match gix::index::fs::Metadata::from_path_no_follow(&file) {
        Ok(md) => md,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Look::Missing;
        }
        Err(_) => return Look::Dirty,
    };
    if md.is_dir() {
        return Look::Missing;
    }
    if !(item.mode == Mode::FILE || item.mode == Mode::FILE_EXECUTABLE) || !md.is_file() {
        return Look::Dirty;
    }
    if cfg!(unix) && md.is_executable() != (item.mode == Mode::FILE_EXECUTABLE) {
        return Look::Dirty;
    }
    let opts = stat_options();
    match Stat::from_fs(&md) {
        Ok(now) if item.stat.matches(&now, opts) && !item.racy => Look::Clean,
        _ => Look::Dirty,
    }
}

fn api_path(path: &BStr) -> String {
    format!("/{}", path.to_str_lossy())
}

fn kind_of(mode: Mode) -> GitEntryKind {
    if mode == Mode::COMMIT {
        GitEntryKind::Submodule
    } else if mode == Mode::SYMLINK {
        GitEntryKind::Symlink
    } else {
        GitEntryKind::File
    }
}

/// A row before it gets its final shape.
struct Row {
    file: GitWorkingFile,
    /// The content the working tree holds, for rename detection (new files only).
    id: Option<gix::ObjectId>,
}

impl Repo {
    /// Every changed file of the whole work tree (see the module documentation).
    pub fn working_copy(&self) -> Result<GitWorkingCopy> {
        let head_commit = self.head_commit()?;
        let head = flatten(&self.repo, self.head_tree()?)?;
        let index = self.load_index()?;
        let timestamp = index.timestamp();

        // Stage 0 entries in path order, and the paths with conflict stages.
        let mut items: Vec<Item<'_>> = Vec::with_capacity(index.entries().len());
        let mut conflicted: Vec<&BStr> = Vec::new();
        let mut all_paths: Vec<&[u8]> = Vec::with_capacity(index.entries().len());
        for entry in index.entries() {
            let path = entry.path(&index);
            all_paths.push(path.as_bytes());
            if entry.stage() == Stage::Unconflicted {
                items.push(Item {
                    path,
                    mode: entry.mode,
                    id: entry.id,
                    stat: entry.stat,
                    racy: entry.stat.is_racy(timestamp, stat_options()),
                });
            } else if conflicted.last() != Some(&path) {
                conflicted.push(path);
            }
        }

        // `lstat` of every tracked file, on several threads.
        let mut looks = vec![Look::Dirty; items.len()];
        let threads = std::thread::available_parallelism()
            .map_or(4, usize::from)
            .clamp(1, 8);
        let chunk = items.len().div_ceil(threads).max(CHUNK);
        let workdir = self.workdir.as_path();
        std::thread::scope(|scope| {
            for (items, looks) in items.chunks(chunk).zip(looks.chunks_mut(chunk)) {
                scope.spawn(move || {
                    for (item, slot) in items.iter().zip(looks.iter_mut()) {
                        if item.mode != Mode::COMMIT {
                            *slot = look(workdir, item);
                        }
                    }
                });
            }
        });

        // Merge join of the index with `HEAD` (both sorted by path): what is staged.
        let mut head_at: Vec<Option<Blob>> = vec![None; items.len()];
        let mut staged_deleted: Vec<(&BString, Blob)> = Vec::new();
        {
            let mut walk = head.iter().peekable();
            for (k, item) in items.iter().enumerate() {
                while let Some((hp, hb)) = walk.peek() {
                    match hp.as_slice().cmp(item.path.as_bytes()) {
                        std::cmp::Ordering::Less => {
                            staged_deleted.push((*hp, **hb));
                            walk.next();
                        }
                        std::cmp::Ordering::Equal => {
                            head_at[k] = Some(**hb);
                            walk.next();
                            break;
                        }
                        std::cmp::Ordering::Greater => break,
                    }
                }
            }
            staged_deleted.extend(walk.map(|(p, b)| (p, *b)));
        }
        staged_deleted.retain(|(p, _)| conflicted.binary_search(&p.as_bstr()).is_err());

        let mut probe = Probe::new(self)?;
        let mut rows: Vec<Row> = Vec::new();
        let mut deleted_ids: HashMap<gix::ObjectId, Vec<usize>> = HashMap::new();

        for (k, item) in items.iter().enumerate() {
            let blob = Blob {
                mode: item.mode,
                id: item.id,
            };
            let staged = match head_at[k] {
                None => Some(GitFileChange::Added),
                Some(h) if !same(h, blob) => Some(GitFileChange::Modified),
                Some(_) => None,
            };
            if item.mode == Mode::COMMIT {
                // A submodule: only what the index records differently from `HEAD`.
                if let Some(change) = staged {
                    rows.push(Row {
                        file: GitWorkingFile {
                            path: api_path(item.path),
                            state: if change == GitFileChange::Added {
                                GitFileState::Untracked
                            } else {
                                GitFileState::Modified
                            },
                            from: None,
                            kind: GitEntryKind::Submodule,
                            staged,
                            unstaged: false,
                            extra: tk_git_types::NoExtra {},
                        },
                        id: None,
                    });
                }
                continue;
            }
            // The working tree against the index (`unstaged`) and against `HEAD` (`state`).
            let (unstaged, now) = match looks[k] {
                Look::Clean => (false, Some(blob)),
                Look::Missing => (true, None),
                Look::Dirty => match probe.state(item.path, &index)? {
                    FileState::File(b) => (!same(blob, b), Some(b)),
                    FileState::Missing | FileState::Dir => (true, None),
                },
            };
            let state = match (head_at[k], now) {
                (Some(h), Some(w)) if same(h, w) => None,
                (Some(_), Some(_)) => Some(GitFileState::Modified),
                (Some(_), None) => Some(GitFileState::Deleted),
                (None, Some(_)) => Some(GitFileState::Untracked),
                // Added to the index and gone again: nothing to commit.
                (None, None) => None,
            };
            let Some(state) = state else { continue };
            if state == GitFileState::Deleted
                && let Some(h) = head_at[k]
            {
                deleted_ids.entry(h.id).or_default().push(rows.len());
            }
            rows.push(Row {
                file: GitWorkingFile {
                    path: api_path(item.path),
                    state,
                    from: None,
                    kind: kind_of(now.map_or(item.mode, |b| b.mode)),
                    staged,
                    unstaged,
                    extra: tk_git_types::NoExtra {},
                },
                id: (state == GitFileState::Untracked)
                    .then_some(now.map(|b| b.id))
                    .flatten(),
            });
        }

        // Staged deletions (`git rm`): still in `HEAD`, not in the index.
        for (path, blob) in &staged_deleted {
            let present = !matches!(
                probe.state(path.as_bstr(), &index)?,
                FileState::Missing | FileState::Dir
            );
            if blob.mode == Mode::COMMIT {
                continue;
            }
            // A file that is still there is found as untracked below.
            if !present {
                deleted_ids.entry(blob.id).or_default().push(rows.len());
                rows.push(Row {
                    file: GitWorkingFile {
                        path: api_path(path.as_bstr()),
                        state: GitFileState::Deleted,
                        from: None,
                        kind: kind_of(blob.mode),
                        staged: Some(GitFileChange::Deleted),
                        unstaged: false,
                        extra: tk_git_types::NoExtra {},
                    },
                    id: None,
                });
            }
        }

        for path in &conflicted {
            rows.push(Row {
                file: GitWorkingFile {
                    path: api_path(path),
                    state: GitFileState::Conflicted,
                    from: None,
                    kind: GitEntryKind::File,
                    staged: None,
                    unstaged: true,
                    extra: tk_git_types::NoExtra {},
                },
                id: None,
            });
        }

        // New files: not in the index, not ignored.
        for path in self.untracked_all(&index, &all_paths)? {
            let in_head = head.get(&path).copied();
            let (state, id) = match in_head {
                // `git rm --cached`: compare with what `HEAD` has.
                Some(h) => match probe.state(path.as_bstr(), &index)? {
                    FileState::File(w) if same(h, w) => (GitFileState::Untracked, Some(w.id)),
                    FileState::File(w) => (GitFileState::Modified, Some(w.id)),
                    _ => continue,
                },
                None => (GitFileState::Untracked, None),
            };
            rows.push(Row {
                file: GitWorkingFile {
                    path: api_path(path.as_bstr()),
                    state,
                    from: None,
                    kind: GitEntryKind::File,
                    staged: in_head.map(|_| GitFileChange::Deleted),
                    unstaged: false,
                    extra: tk_git_types::NoExtra {},
                },
                id,
            });
        }

        // Renames: a deleted file whose content shows up under a new name. The deleted row
        // stays (committing the rename needs both paths); the new row names the old one.
        if !deleted_ids.is_empty() {
            let new_rows: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.file.state == GitFileState::Untracked && r.file.from.is_none())
                .map(|(i, _)| i)
                .collect();
            if new_rows.len() <= RENAME_CANDIDATES {
                let mut taken: HashSet<usize> = HashSet::new();
                for i in new_rows {
                    let id = match rows[i].id {
                        Some(id) => Some(id),
                        None => self.hash_file(&mut probe, &index, &rows[i].file.path)?,
                    };
                    let Some(id) = id else { continue };
                    let Some(old) = deleted_ids
                        .get(&id)
                        .and_then(|c| c.iter().copied().find(|j| !taken.contains(j)))
                    else {
                        continue;
                    };
                    taken.insert(old);
                    let from = rows[old].file.path.clone();
                    let staged_pair = rows[i].file.staged == Some(GitFileChange::Added)
                        && rows[old].file.staged == Some(GitFileChange::Deleted);
                    rows[i].file.from = Some(from);
                    if staged_pair {
                        rows[i].file.staged = Some(GitFileChange::Renamed);
                    }
                }
            }
        }

        let mut files: Vec<GitWorkingFile> = rows.into_iter().map(|r| r.file).collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let truncated = files.len() > WORKING_COPY_LIMIT;
        files.truncate(WORKING_COPY_LIMIT);

        let (upstream, ahead, behind) = match self.upstream()? {
            Some(up) => {
                let (ahead, behind) = self.ahead_behind(&up.tracking)?;
                (Some(up.display), ahead, behind)
            }
            None => (None, 0, 0),
        };
        let branch = match head_commit {
            Some(_) => self.head_branch()?,
            None => None,
        };
        Ok(GitWorkingCopy {
            repository: self.workdir.to_string_lossy().into_owned(),
            detached: head_commit.is_some() && self.head_ref()?.is_none(),
            branch,
            upstream,
            ahead,
            behind,
            head: head_commit.map(|c| c.to_string()),
            operation: self.operation()?,
            files,
            truncated,
        })
    }

    /// What is half done, for [`GitWorkingCopy::operation`].
    fn operation(&self) -> Result<Option<GitOperation>> {
        Ok(match self.underway()? {
            Some(Underway::Merge) => Some(GitOperation::Merge),
            Some(Underway::CherryPick) => Some(GitOperation::CherryPick),
            Some(Underway::Revert) => Some(GitOperation::Revert),
            None => {
                let dir = self.git_dir();
                (dir.join("rebase-merge").is_dir() || dir.join("rebase-apply").is_dir())
                    .then_some(GitOperation::Rebase)
            }
        })
    }

    /// The id of the file `api_path` (leading `/`) as the working tree holds it; `None` if it
    /// is not a readable regular file below the size limit.
    fn hash_file(
        &self,
        probe: &mut Probe<'_>,
        index: &gix::index::State,
        api_path: &str,
    ) -> Result<Option<gix::ObjectId>> {
        let rel = api_path.strip_prefix('/').unwrap_or(api_path);
        let file = self.workdir.join(rel);
        match std::fs::symlink_metadata(&file) {
            Ok(md) if md.is_file() && md.len() <= BLOB_TEXT_LIMIT => {}
            _ => return Ok(None),
        }
        Ok(match probe.state(rel.as_bytes().as_bstr(), index)? {
            FileState::File(b) => Some(b.id),
            _ => None,
        })
    }

    /// Files of the whole work tree that are neither tracked nor ignored (repository paths).
    /// `known` are all index paths in order. Folders are read by several threads, each with its
    /// own ignore stack (the stack is cheapest when it stays in one place, so a thread keeps
    /// working below the folder it was in).
    fn untracked_all(&self, index: &gix::index::State, known: &[&[u8]]) -> Result<Vec<BString>> {
        let names: HashSet<&[u8]> = known.iter().copied().collect();
        let (workdir, options) = (self.workdir.as_path(), &self.options);
        let queue = Queue::new(Task {
            dir: self.workdir.clone(),
            rel: BString::default(),
            excluded: false,
        });
        let threads = std::thread::available_parallelism()
            .map_or(4, usize::from)
            .clamp(1, 8);
        let results: Vec<Result<Vec<BString>>> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        // Each thread opens the repository itself: gix is built single-threaded.
                        let repo = gix::open_opts(workdir, options.open())
                            .ctx("cannot open the Git repository")?;
                        let mut excludes = repo
                            .excludes(
                                index,
                                None,
                                gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
                            )
                            .ctx("cannot read .gitignore")?;
                        let mut out = Vec::new();
                        let mut failed = None;
                        while let Some(task) = queue.take() {
                            if failed.is_none()
                                && let Err(e) =
                                    walk_dir(&task, &mut excludes, &names, known, &queue, &mut out)
                            {
                                failed = Some(e);
                            }
                            queue.done();
                        }
                        failed.map_or(Ok(out), Err)
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|w| {
                    w.join()
                        .unwrap_or_else(|_| Err(fail("a Git worker panicked")))
                })
                .collect()
        });
        let mut out = Vec::new();
        for r in results {
            out.extend(r?);
        }
        Ok(out)
    }
}

/// A folder to look into.
struct Task {
    dir: std::path::PathBuf,
    rel: BString,
    /// A parent folder is ignored: everything below is, unless tracked.
    excluded: bool,
}

/// Folders still to read, shared by the workers.
struct Queue {
    state: std::sync::Mutex<(Vec<Task>, usize)>,
    ready: std::sync::Condvar,
}

impl Queue {
    fn new(first: Task) -> Self {
        Self {
            state: std::sync::Mutex::new((vec![first], 0)),
            ready: std::sync::Condvar::new(),
        }
    }

    fn push(&self, task: Task) {
        if let Ok(mut s) = self.state.lock() {
            s.0.push(task);
            self.ready.notify_one();
        }
    }

    /// The next folder, waiting while other workers may still add some; `None` when all work
    /// is done.
    fn take(&self) -> Option<Task> {
        let mut s = self.state.lock().ok()?;
        loop {
            if let Some(task) = s.0.pop() {
                s.1 += 1;
                return Some(task);
            }
            if s.1 == 0 {
                self.ready.notify_all();
                return None;
            }
            s = self.ready.wait(s).ok()?;
        }
    }

    fn done(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.1 -= 1;
            if s.1 == 0 && s.0.is_empty() {
                self.ready.notify_all();
            }
        }
    }
}

/// Reads one folder: new files go to `out`, subfolders to the queue.
fn walk_dir(
    task: &Task,
    excludes: &mut gix::AttributeStack<'_>,
    names: &HashSet<&[u8]>,
    known: &[&[u8]],
    queue: &Queue,
    out: &mut Vec<BString>,
) -> Result<()> {
    // Is anything tracked below `dir`?
    let tracked_below = |dir: &[u8]| {
        let mut start = dir.to_vec();
        start.push(b'/');
        let at = known.partition_point(|p| *p < start.as_slice());
        known.get(at).is_some_and(|p| p.starts_with(&start))
    };
    let entries = std::fs::read_dir(&task.dir).ctx("cannot read a folder")?;
    for entry in entries {
        let entry = entry.ctx("cannot read a folder")?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name == ".git" {
            continue;
        }
        let mut path = task.rel.clone();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.push_str(name.as_bytes());
        let kind = entry.file_type().ctx("cannot read a folder")?;
        if kind.is_dir() {
            // A submodule's checkout, or a nested repository: not ours to list.
            if names.contains(path.as_slice()) {
                continue;
            }
            let below = tracked_below(path.as_slice());
            if !below && entry.path().join(".git").exists() {
                continue;
            }
            let excluded = task.excluded
                || excludes
                    .at_entry(path.as_bstr(), Some(Mode::DIR))
                    .ctx("cannot read .gitignore")?
                    .is_excluded();
            if excluded && !below {
                continue;
            }
            queue.push(Task {
                dir: entry.path(),
                rel: path,
                excluded,
            });
        } else if !names.contains(path.as_slice()) {
            let excluded = task.excluded
                || excludes
                    .at_entry(path.as_bstr(), Some(Mode::FILE))
                    .ctx("cannot read .gitignore")?
                    .is_excluded();
            if !excluded {
                out.push(path);
            }
        }
    }
    Ok(())
}

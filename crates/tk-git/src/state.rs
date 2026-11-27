// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The state of a working tree as Git objects, and setting it back: the building blocks of a
//! product's time machine, of an undo through a snapshot and of parking changes per branch.
//!
//! - [`Repo::ref_state`] reads every ref and `HEAD` ([`RefState`], text form
//!   [`RefState::to_text`]); the product's own namespace (and those it reads) is left out.
//! - [`Repo::capture_working_state`] writes the working files, the index (stage 0), its
//!   conflict stages and the files of a merge in progress as trees ([`WorkingState`]). Only
//!   blobs and trees are written: `.git/index`, the working files and every ref stay as they
//!   are (no stat refresh, no lock).
//! - [`Repo::write_record_tree`] and [`Repo::write_record_commit`] put such trees and small
//!   files into one commit; [`Repo::own_refs`] and [`Repo::set_own_ref`] keep refs under the
//!   product's namespace (compare and swap).
//! - [`Repo::state_preview`] says what [`Repo::set_state`] would change (reads only);
//!   [`Repo::set_state`] sets refs, `HEAD`, the index, the merge files and the working files
//!   to a [`StateTarget`], with the expected values the caller saw ([`StateExpected`]): when a
//!   ref moved or a file to change was edited since, it refuses and changes nothing.
//!
//! Layout of a conflicts tree: `1/<path>`, `2/<path>`, `3/<path>` (base, ours, theirs; blob
//! and mode as in the index). A merge tree holds the files of [`MERGE_FILES`] that exist, by
//! name, bytes as in the Git folder.

use std::collections::{BTreeMap, BTreeSet};

use gix::ObjectId;
use gix::bstr::{BStr, BString, ByteSlice};
use gix::index::entry::{Flags, Mode, Stage};
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};
use serde::{Deserialize, Serialize};
use tk_git_types::actions::GitRefMove;
use tk_git_types::graph::GitChangedFile;
use tk_git_types::{GitDiff, GitFile, GitFileChange, GitFileState, text_diff};

use crate::commit::entry_kind;
use crate::repo::Repo;
use crate::util::{Blob, Context, Flat, Result, blob_bytes, fail, flatten};
use crate::worktree::{FileState, Probe, blocked, blocked_error, switch};

/// The files of a merge, cherry-pick or revert in progress, in the Git folder.
pub const MERGE_FILES: [&str; 5] = [
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "MERGE_MSG",
    "MERGE_MODE",
];

/// What a ref (or `HEAD`) holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum RefValue {
    /// An object id (hex): a commit, or a tag object for an annotated tag.
    Id(String),
    /// Another ref by full name (`HEAD` on a branch, `refs/remotes/origin/HEAD`).
    Symbolic(String),
}

impl RefValue {
    fn text(&self) -> String {
        match self {
            Self::Id(id) => id.clone(),
            Self::Symbolic(name) => format!("ref: {name}"),
        }
    }

    fn parse(text: &str) -> Result<Self> {
        match text.strip_prefix("ref: ") {
            Some(name) => Ok(Self::Symbolic(name.to_owned())),
            None if ObjectId::from_hex(text.as_bytes()).is_ok() => Ok(Self::Id(text.to_owned())),
            None => Err(fail(format!("invalid ref value {text:?}"))),
        }
    }
}

/// Every ref and `HEAD`, without the product's namespace and the namespaces it reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefState {
    pub head: RefValue,
    /// By full name (`refs/…`).
    pub refs: BTreeMap<String, RefValue>,
}

impl RefState {
    /// One line per ref, `<value> <name>`: `HEAD` first, then the refs sorted by name; a value
    /// is an id or `ref: <name>`. For refs that hold ids this is what
    /// `git for-each-ref --format='%(objectname) %(refname)'` prints.
    pub fn to_text(&self) -> String {
        let mut out = format!("{} HEAD\n", self.head.text());
        for (name, value) in &self.refs {
            out.push_str(&format!("{} {name}\n", value.text()));
        }
        out
    }

    /// The inverse of [`RefState::to_text`].
    pub fn parse(text: &str) -> Result<Self> {
        let mut head = None;
        let mut refs = BTreeMap::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let (value, name) = line
                .rsplit_once(' ')
                .ok_or_else(|| fail(format!("invalid ref line {line:?}")))?;
            let value = RefValue::parse(value)?;
            if name == "HEAD" {
                head = Some(value);
            } else {
                refs.insert(name.to_owned(), value);
            }
        }
        Ok(Self {
            head: head.ok_or_else(|| fail("the ref state has no HEAD"))?,
            refs,
        })
    }

    /// The branch `HEAD` is on (full name), also when it has no commit yet.
    pub fn head_branch(&self) -> Option<&str> {
        match &self.head {
            RefValue::Symbolic(name) => Some(name),
            RefValue::Id(_) => None,
        }
    }

    /// The commit `HEAD` points to (following one symbolic step); `None` when unborn.
    pub fn head_id(&self) -> Option<&str> {
        match &self.head {
            RefValue::Id(id) => Some(id),
            RefValue::Symbolic(name) => match self.refs.get(name) {
                Some(RefValue::Id(id)) => Some(id),
                _ => None,
            },
        }
    }
}

/// How [`Repo::capture_working_state`] captures.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaptureOptions {
    /// Files larger than this stay out of [`WorkingState::files`] (their `HEAD` version, if
    /// any, stays in) and are listed in [`WorkingState::skipped`].
    pub max_file_bytes: Option<u64>,
}

/// A working tree written as Git objects. Ids are hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingState {
    /// The commit `HEAD` points to; `None` when unborn.
    pub head: Option<String>,
    /// The working files: `HEAD`'s tree with every changed, deleted, conflicted and untracked
    /// (not ignored) file as it is on disk (clean filters applied, as `git add` would).
    pub files: String,
    /// The index, stage 0 (entries marked intent-to-add are left out, as `git write-tree`
    /// does).
    pub index: String,
    /// The conflict stages (`1/`, `2/`, `3/`); `None` without unmerged entries.
    pub conflicts: Option<String>,
    /// The files of a merge in progress ([`MERGE_FILES`]); `None` when none runs.
    pub merge: Option<String>,
    /// Paths that differ from `HEAD` (repository-relative, leading `/`).
    pub changed: Vec<String>,
    /// Paths left out for their size (leading `/`).
    pub skipped: Vec<String>,
}

/// One entry of [`Repo::write_record_tree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordEntry {
    /// A file (`100644`) with these bytes.
    Blob { name: String, bytes: Vec<u8> },
    /// A folder: an existing tree.
    Tree { name: String, id: String },
}

/// What [`Repo::set_state`] sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTarget {
    /// Refs and `HEAD`; `None`: they stay as they are. `refs/stash` is never moved (its
    /// reflog is the stash list).
    pub refs: Option<RefState>,
    /// The working files (a tree).
    pub files: String,
    /// The index, stage 0 (a tree).
    pub index: String,
    /// The conflict stages, see the module docs.
    pub conflicts: Option<String>,
    /// The merge files; `None`: no merge in progress (they are removed).
    pub merge: Option<String>,
    /// Paths (leading `/`) whose working file is never touched (left out of a snapshot for
    /// their size, now or then).
    pub keep: Vec<String>,
}

/// What the caller saw when it decided to set the state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateExpected {
    /// The refs as they are now; `None`: not checked (only with [`StateTarget::refs`] `None`).
    pub refs: Option<RefState>,
    /// The working files as they are now (a tree, normally [`WorkingState::files`] of a
    /// capture just taken).
    pub files: String,
}

/// What setting a state changes (or changed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChange {
    /// Ref moves with real ids; `HEAD` with the commits it points to before and after.
    pub refs: Vec<GitRefMove>,
    /// `HEAD` changes branch (or goes detached): (before, after), full names or ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<(String, String)>,
    /// Refs of the target whose object is gone (`git gc`): left as they are.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    /// Working files from now to the target.
    pub files: Vec<GitChangedFile>,
    /// The index differs from the target.
    pub index: bool,
    /// The merge files differ from the target.
    pub merge: bool,
}

fn oid(hex: &str) -> Result<ObjectId> {
    ObjectId::from_hex(hex.trim().as_bytes()).map_err(|_| fail(format!("{hex:?} is not an id")))
}

fn shown(path: &BStr) -> String {
    format!("/{}", path.to_str_lossy())
}

fn rel(path: &str) -> BString {
    BString::from(path.strip_prefix('/').unwrap_or(path))
}

fn full_name(name: &str) -> Result<FullName> {
    FullName::try_from(name).map_err(|_| fail(format!("invalid ref {name}")))
}

fn target_of(value: &RefValue) -> Result<Target> {
    Ok(match value {
        RefValue::Id(id) => Target::Object(oid(id)?),
        RefValue::Symbolic(name) => Target::Symbolic(full_name(name)?),
    })
}

fn expected_of(value: Option<&RefValue>) -> Result<PreviousValue> {
    Ok(match value {
        Some(v) => PreviousValue::MustExistAndMatch(target_of(v)?),
        None => PreviousValue::MustNotExist,
    })
}

impl Repo {
    /// Whether `name` lies in the product's namespace or one it reads.
    fn is_own(&self, name: &str) -> bool {
        let ns = self.namespace();
        std::iter::once(ns.name())
            .chain(ns.reads().iter().copied())
            .any(|n| name.starts_with(&format!("refs/{n}/")))
    }

    /// Where the product keeps its files for this working tree (`.git/<namespace>` of the main
    /// working tree, `.git/worktrees/<id>/<namespace>` of a linked one; the action journal
    /// lives here).
    pub fn own_dir(&self) -> std::path::PathBuf {
        self.git_dir().join(self.namespace().name())
    }

    /// Where the product keeps its files for the whole repository (`.git/<namespace>`).
    pub fn own_common_dir(&self) -> std::path::PathBuf {
        let common = self.repo.common_dir();
        crate::util::canonical(common)
            .unwrap_or_else(|_| common.to_path_buf())
            .join(self.namespace().name())
    }

    /// The id of a linked working tree (its folder name in `.git/worktrees`); `None` for the
    /// main one.
    pub fn worktree_id(&self) -> Option<String> {
        let wt = self.repo.worktree()?;
        wt.id()
            .ok()
            .flatten()
            .map(|id| id.to_str_lossy().into_owned())
    }

    /// Every ref and `HEAD` (see [`RefState`]).
    pub fn ref_state(&self) -> Result<RefState> {
        let head = match self.head_ref()? {
            Some(name) => RefValue::Symbolic(name.as_bstr().to_str_lossy().into_owned()),
            None => RefValue::Id(
                self.head_commit()?
                    .ok_or_else(|| fail("HEAD points nowhere"))?
                    .to_string(),
            ),
        };
        let mut refs = BTreeMap::new();
        let platform = self.repo.references().ctx("cannot list the refs")?;
        for r in platform.all().ctx("cannot list the refs")? {
            let r = r.map_err(|e| fail(format!("cannot list the refs: {e}")))?;
            let name = r.name().as_bstr().to_str_lossy().into_owned();
            if !name.starts_with("refs/") || self.is_own(&name) {
                continue;
            }
            let value = match r.target() {
                gix::refs::TargetRef::Object(id) => RefValue::Id(id.to_string()),
                gix::refs::TargetRef::Symbolic(to) => {
                    RefValue::Symbolic(to.as_bstr().to_str_lossy().into_owned())
                }
            };
            refs.insert(name, value);
        }
        Ok(RefState { head, refs })
    }

    /// Writes the working tree as objects (see [`WorkingState`]); nothing else changes.
    pub fn capture_working_state(&self, options: &CaptureOptions) -> Result<WorkingState> {
        let whole = self.whole()?;
        let status = whole.status()?.files;
        let mut take: Vec<&GitFile> = Vec::new();
        let mut skipped = Vec::new();
        for f in &status {
            if f.state != GitFileState::Deleted
                && let Some(max) = options.max_file_bytes
            {
                let path = crate::worktree::fs_path(&whole.workdir, f.path.as_bytes().as_bstr())?;
                if let Ok(md) = std::fs::symlink_metadata(&path)
                    && md.is_file()
                    && md.len() > max
                {
                    skipped.push(format!("/{}", f.path));
                    continue;
                }
            }
            take.push(f);
        }
        let head_tree = whole.head_tree()?;
        let files = crate::undo::tree_with(&whole, head_tree, &take)?;
        let index = whole.load_index()?;
        let index_tree = whole.index_tree(&index, head_tree)?;
        let conflicts = whole.conflicts_tree(&index)?;
        let merge = whole.merge_tree()?;
        Ok(WorkingState {
            head: whole.head_commit()?.map(|c| c.to_string()),
            files: files.to_string(),
            index: index_tree.to_string(),
            conflicts: conflicts.map(|c| c.to_string()),
            merge: merge.map(|m| m.to_string()),
            changed: status
                .iter()
                .map(|f| format!("/{}", f.path))
                .filter(|p| !skipped.contains(p))
                .collect(),
            skipped,
        })
    }

    /// The stage-0 entries of `index` as a tree, edited from `base` (usually `HEAD`'s tree,
    /// so only what differs is touched).
    fn index_tree(&self, index: &gix::index::State, base: ObjectId) -> Result<ObjectId> {
        let head = flatten(&self.repo, base)?;
        let mut editor = self.repo.edit_tree(base).ctx("cannot read a tree")?;
        let mut seen = BTreeSet::new();
        let mut edited = false;
        for e in index.entries() {
            if e.stage() != Stage::Unconflicted || e.flags.contains(Flags::INTENT_TO_ADD) {
                continue;
            }
            let path = e.path(index);
            seen.insert(path.to_owned());
            let blob = Blob {
                mode: e.mode,
                id: e.id,
            };
            if head.get(path) != Some(&blob) {
                editor
                    .upsert(path, entry_kind(e.mode), e.id)
                    .ctx("cannot build the tree")?;
                edited = true;
            }
        }
        for path in head.keys() {
            if !seen.contains(path) {
                editor.remove(path.as_bstr()).ctx("cannot build the tree")?;
                edited = true;
            }
        }
        if !edited {
            return Ok(base);
        }
        Ok(editor.write().ctx("cannot write the tree")?.detach())
    }

    /// The unmerged entries as `1/`, `2/`, `3/`; `None` without any.
    fn conflicts_tree(&self, index: &gix::index::State) -> Result<Option<ObjectId>> {
        let empty = ObjectId::empty_tree(self.repo.object_hash());
        let mut editor = self.repo.edit_tree(empty).ctx("cannot build the tree")?;
        let mut any = false;
        for e in index.entries() {
            let stage = match e.stage() {
                Stage::Unconflicted => continue,
                Stage::Base => "1",
                Stage::Ours => "2",
                Stage::Theirs => "3",
            };
            let mut path = BString::from(format!("{stage}/"));
            path.extend_from_slice(e.path(index));
            editor
                .upsert(path.as_bstr(), entry_kind(e.mode), e.id)
                .ctx("cannot build the tree")?;
            any = true;
        }
        if !any {
            return Ok(None);
        }
        Ok(Some(editor.write().ctx("cannot write the tree")?.detach()))
    }

    /// The merge files that exist, as a tree; `None` when none does.
    fn merge_tree(&self) -> Result<Option<ObjectId>> {
        let mut entries = Vec::new();
        for name in MERGE_FILES {
            match std::fs::read(self.git_dir().join(name)) {
                Ok(bytes) => entries.push(RecordEntry::Blob {
                    name: name.to_owned(),
                    bytes,
                }),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(fail(format!("cannot read {name}: {e}"))),
            }
        }
        if entries.is_empty() {
            return Ok(None);
        }
        Ok(Some(oid(&self.write_record_tree(&entries)?)?))
    }

    /// A tree of `entries` (names without `/`), written.
    pub fn write_record_tree(&self, entries: &[RecordEntry]) -> Result<String> {
        let empty = ObjectId::empty_tree(self.repo.object_hash());
        let mut editor = self.repo.edit_tree(empty).ctx("cannot build the tree")?;
        for e in entries {
            let (name, kind, id) = match e {
                RecordEntry::Blob { name, bytes } => (
                    name,
                    gix::objs::tree::EntryKind::Blob,
                    self.repo
                        .write_blob(bytes)
                        .ctx("cannot write a file")?
                        .detach(),
                ),
                RecordEntry::Tree { name, id } => {
                    (name, gix::objs::tree::EntryKind::Tree, oid(id)?)
                }
            };
            if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                return Err(fail(format!("{name:?} is not a file name")));
            }
            editor
                .upsert(name.as_bytes().as_bstr(), kind, id)
                .ctx("cannot build the tree")?;
        }
        Ok(editor.write().ctx("cannot write the tree")?.to_string())
    }

    /// A commit of `tree` with `parents` (ids), by the product's own identity, now. No ref
    /// points to it until the caller sets one.
    pub fn write_record_commit(
        &self,
        tree: &str,
        parents: &[String],
        message: &str,
    ) -> Result<String> {
        let parents = parents.iter().map(|p| oid(p)).collect::<Result<Vec<_>>>()?;
        Ok(crate::undo::write_commit(self, oid(tree)?, parents, message, None)?.to_string())
    }

    /// The object at `path` (`/`-separated, no leading `/`) in the tree of `rev` (a commit or
    /// tree id): (id, is a tree); `None` when absent.
    pub fn record_entry(&self, rev: &str, path: &str) -> Result<Option<(String, bool)>> {
        let object = self
            .repo
            .find_object(oid(rev)?)
            .ctx("cannot read an object")?;
        let tree = object.peel_to_tree().ctx("cannot read a tree")?;
        let Some(entry) = tree.lookup_entry_by_path(path).ctx("cannot read a tree")? else {
            return Ok(None);
        };
        Ok(Some((entry.oid().to_string(), entry.mode().is_tree())))
    }

    /// The bytes of the file at `path` in the tree of `rev` (see [`Repo::record_entry`]).
    pub fn record_blob(&self, rev: &str, path: &str) -> Result<Option<Vec<u8>>> {
        match self.record_entry(rev, path)? {
            Some((id, false)) => Ok(Some(blob_bytes(&self.repo, oid(&id)?)?)),
            _ => Ok(None),
        }
    }

    /// Whether the object `id` exists.
    pub fn has_object(&self, id: &str) -> bool {
        oid(id).is_ok_and(|id| self.repo.has_object(id))
    }

    /// The refs under `refs/<namespace>/<prefix>` (full name, id), sorted by name.
    pub fn own_refs(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let full = format!("refs/{}/{prefix}", self.namespace().name());
        let mut out = Vec::new();
        let platform = self.repo.references().ctx("cannot list the refs")?;
        for r in platform
            .prefixed(full.as_str())
            .ctx("cannot list the refs")?
        {
            let r = r.map_err(|e| fail(format!("cannot list the refs: {e}")))?;
            if let Some(id) = r.target().try_id() {
                out.push((
                    r.name().as_bstr().to_str_lossy().into_owned(),
                    id.to_string(),
                ));
            }
        }
        out.sort();
        Ok(out)
    }

    /// Sets (`Some`) or deletes (`None`) a ref under `refs/<namespace>/`, if it still is
    /// `expected` (`None`: must not exist). No reflog.
    pub fn set_own_ref(&self, name: &str, new: Option<&str>, expected: Option<&str>) -> Result<()> {
        if !name.starts_with(&format!("refs/{}/", self.namespace().name())) {
            return Err(fail(format!("{name} is not in the product's namespace")));
        }
        let full = full_name(name)?;
        let prev = match expected {
            Some(id) => PreviousValue::MustExistAndMatch(Target::Object(oid(id)?)),
            None => PreviousValue::MustNotExist,
        };
        let change = match new {
            // Logged only where `core.logAllRefUpdates` says so, which never covers the
            // product's namespace: own refs keep no reflog.
            Some(id) => Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: BString::default(),
                },
                expected: prev,
                new: Target::Object(oid(id)?),
            },
            None if expected.is_none() => return Ok(()),
            None => Change::Delete {
                expected: prev,
                log: RefLog::AndReference,
            },
        };
        self.repo
            .edit_reference(RefEdit {
                change,
                name: full,
                deref: false,
            })
            .ctx(&format!("cannot update {name}"))?;
        Ok(())
    }

    /// The working files as they are now, in memory (nothing is written): `HEAD`'s files with
    /// every changed, deleted and untracked file as hashed from disk.
    fn current_files(&self) -> Result<Flat> {
        let mut flat = flatten(&self.repo, self.head_tree()?)?;
        let index = self.load_index()?;
        let mut probe = Probe::new(self)?;
        for f in self.status()?.files {
            let path = BString::from(f.path.as_str());
            match probe.state(path.as_bstr(), &index)? {
                FileState::File(b) => {
                    flat.insert(path, b);
                }
                FileState::Missing | FileState::Dir => {
                    flat.remove(&path);
                }
            }
        }
        Ok(flat)
    }

    /// `to` with the `keep` paths as they are in `from` (absent there: absent).
    fn with_kept(&self, from: &Flat, to: ObjectId, keep: &[String]) -> Result<ObjectId> {
        if keep.is_empty() {
            return Ok(to);
        }
        let mut editor = self.repo.edit_tree(to).ctx("cannot read a tree")?;
        for p in keep {
            let path = rel(p);
            match from.get(&path) {
                Some(b) => editor
                    .upsert(path.as_bstr(), entry_kind(b.mode), b.id)
                    .ctx("cannot build the tree")?,
                None => editor.remove(path.as_bstr()).ctx("cannot build the tree")?,
            };
        }
        Ok(editor.write().ctx("cannot write the tree")?.detach())
    }

    /// Ref edits from `now` to `target` (deletions separate), the moves and the missing refs.
    #[allow(clippy::type_complexity)]
    fn ref_edits(
        &self,
        now: &RefState,
        target: &RefState,
    ) -> Result<(
        Vec<RefEdit>,
        Vec<RefEdit>,
        Vec<GitRefMove>,
        Option<(String, String)>,
        Vec<String>,
    )> {
        let message: BString = self.reflog_message("restore").into();
        let log = || LogChange {
            mode: RefLog::AndReference,
            force_create_reflog: false,
            message: message.clone(),
        };
        let peel = |state: &RefState, v: &RefValue| -> Option<String> {
            match v {
                RefValue::Id(id) => Some(id.clone()),
                RefValue::Symbolic(n) => match state.refs.get(n) {
                    Some(RefValue::Id(id)) => Some(id.clone()),
                    _ => None,
                },
            }
        };
        let (mut deletes, mut updates, mut moves, mut missing) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let names: BTreeSet<&String> = now.refs.keys().chain(target.refs.keys()).collect();
        for name in names {
            if name == "refs/stash" {
                continue;
            }
            let (before, after) = (now.refs.get(name), target.refs.get(name));
            if before == after {
                continue;
            }
            if let Some(RefValue::Id(id)) = after
                && !self.has_object(id)
            {
                missing.push(name.clone());
                continue;
            }
            let full = full_name(name)?;
            moves.push(GitRefMove {
                name: name.clone(),
                before: before.and_then(|v| peel(now, v)),
                after: after.and_then(|v| peel(target, v)),
            });
            match after {
                None => deletes.push(RefEdit {
                    change: Change::Delete {
                        expected: expected_of(before)?,
                        log: RefLog::AndReference,
                    },
                    name: full,
                    deref: false,
                }),
                Some(v) => updates.push(RefEdit {
                    change: Change::Update {
                        log: log(),
                        expected: expected_of(before)?,
                        new: target_of(v)?,
                    },
                    name: full,
                    deref: false,
                }),
            }
        }
        let mut head = None;
        if now.head != target.head {
            let ok = match &target.head {
                RefValue::Id(id) => self.has_object(id),
                RefValue::Symbolic(_) => true,
            };
            if ok {
                updates.push(RefEdit {
                    change: Change::Update {
                        log: log(),
                        expected: expected_of(Some(&now.head))?,
                        new: target_of(&target.head)?,
                    },
                    name: "HEAD".try_into().expect("valid"),
                    deref: false,
                });
                head = Some((now.head.text(), target.head.text()));
            } else {
                missing.push("HEAD".into());
            }
        }
        let (hb, ha) = (now.head_id(), target.head_id());
        if hb != ha && !missing.iter().any(|m| m == "HEAD") {
            moves.push(GitRefMove {
                name: "HEAD".into(),
                before: hb.map(str::to_owned),
                after: ha.map(str::to_owned),
            });
        }
        Ok((deletes, updates, moves, head, missing))
    }

    /// The working files from `from` to `to`, sorted by path (no rename detection).
    fn file_changes(&self, from: &Flat, to: &Flat) -> Result<Vec<GitChangedFile>> {
        let paths: BTreeSet<&BString> = from.keys().chain(to.keys()).collect();
        let mut out = Vec::new();
        for p in paths {
            let (a, b) = (from.get(p), to.get(p));
            if a == b {
                continue;
            }
            let change = match (a, b) {
                (None, _) => GitFileChange::Added,
                (_, None) => GitFileChange::Deleted,
                _ => GitFileChange::Modified,
            };
            let binary = [a, b].into_iter().flatten().any(|blob| {
                blob.mode == Mode::COMMIT
                    || self.repo.find_blob(blob.id).is_ok_and(|o| {
                        let d = &o.data[..o.data.len().min(8000)];
                        d.contains(&0)
                    })
            });
            out.push(GitChangedFile {
                path: shown(p.as_bstr()),
                change,
                from: None,
                binary,
                extra: Default::default(),
            });
        }
        Ok(out)
    }

    fn index_differs(
        &self,
        index: &gix::index::State,
        target_index: ObjectId,
        conflicts: Option<ObjectId>,
    ) -> Result<bool> {
        let want = self.target_entries(target_index, conflicts)?;
        let mut have: Vec<(BString, u8, Mode, ObjectId)> = index
            .entries()
            .iter()
            .filter(|e| !e.flags.contains(Flags::INTENT_TO_ADD))
            .map(|e| (e.path(index).to_owned(), e.stage() as u8, e.mode, e.id))
            .collect();
        have.sort();
        Ok(have != want)
    }

    /// The index entries of a target: (path, stage, mode, id), sorted.
    fn target_entries(
        &self,
        index: ObjectId,
        conflicts: Option<ObjectId>,
    ) -> Result<Vec<(BString, u8, Mode, ObjectId)>> {
        let mut out: Vec<_> = flatten(&self.repo, index)?
            .into_iter()
            .map(|(p, b)| (p, 0u8, b.mode, b.id))
            .collect();
        if let Some(c) = conflicts {
            for (p, b) in flatten(&self.repo, c)? {
                let Some((stage, path)) = p.split_once_str(b"/") else {
                    continue;
                };
                let stage = match stage {
                    b"1" => 1,
                    b"2" => 2,
                    b"3" => 3,
                    _ => continue,
                };
                out.push((BString::from(path), stage, b.mode, b.id));
            }
        }
        out.sort();
        Ok(out)
    }

    fn merge_differs(&self, target: Option<&str>) -> Result<bool> {
        let now = self.merge_tree_text()?;
        let want = match target {
            Some(t) => self.merge_files_of(oid(t)?)?,
            None => BTreeMap::new(),
        };
        Ok(now != want)
    }

    fn merge_tree_text(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut out = BTreeMap::new();
        for name in MERGE_FILES {
            if let Ok(bytes) = std::fs::read(self.git_dir().join(name)) {
                out.insert(name.to_owned(), bytes);
            }
        }
        Ok(out)
    }

    fn merge_files_of(&self, tree: ObjectId) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut out = BTreeMap::new();
        for (p, b) in flatten(&self.repo, tree)? {
            let name = p.to_str_lossy().into_owned();
            if MERGE_FILES.contains(&name.as_str()) {
                out.insert(name, blob_bytes(&self.repo, b.id)?);
            }
        }
        Ok(out)
    }

    /// What [`Repo::set_state`] would change now (reads only; nothing is written).
    pub fn state_preview(&self, target: &StateTarget) -> Result<StateChange> {
        let whole = self.whole()?;
        let from = whole.current_files()?;
        let to = whole.with_kept_flat(&from, oid(&target.files)?, &target.keep)?;
        let (refs, head, missing) = match &target.refs {
            Some(t) => {
                let (_, _, moves, head, missing) = whole.ref_edits(&whole.ref_state()?, t)?;
                (moves, head, missing)
            }
            None => (Vec::new(), None, Vec::new()),
        };
        let index = whole.load_index()?;
        Ok(StateChange {
            refs,
            head,
            missing,
            files: whole.file_changes(&from, &to)?,
            index: whole.index_differs(
                &index,
                oid(&target.index)?,
                target.conflicts.as_deref().map(oid).transpose()?,
            )?,
            merge: whole.merge_differs(target.merge.as_deref())?,
        })
    }

    /// The flattened `to` with the `keep` paths as in `from`, in memory.
    fn with_kept_flat(&self, from: &Flat, to: ObjectId, keep: &[String]) -> Result<Flat> {
        let mut to = flatten(&self.repo, to)?;
        for p in keep {
            let path = rel(p);
            match from.get(&path) {
                Some(b) => {
                    to.insert(path, *b);
                }
                None => {
                    to.remove(&path);
                }
            }
        }
        Ok(to)
    }

    /// The diff of one working file (leading `/`) from disk to the tree `files`.
    pub fn state_file_diff(&self, files: &str, path: &str) -> Result<GitDiff> {
        let whole = self.whole()?;
        let p = rel(path);
        if p.is_empty()
            || p.split_str("/")
                .any(|s| s.is_empty() || s == b".." || s == b".git")
        {
            return Err(fail(format!("{path:?} is not a path in the repository")));
        }
        let file = crate::worktree::fs_path(&whole.workdir, p.as_bstr())?;
        let old = match std::fs::symlink_metadata(&file) {
            Ok(md) if md.is_file() => Some(std::fs::read(&file).ctx("cannot read a file")?),
            _ => None,
        };
        let new = match whole.record_entry(files, &p.to_str_lossy())? {
            Some((id, false)) => Some(blob_bytes(&whole.repo, oid(&id)?)?),
            _ => None,
        };
        Ok(text_diff(
            &shown(p.as_bstr()),
            old.as_deref(),
            new.as_deref(),
        ))
    }

    /// Sets the state (see the module docs): first checks that the refs are still
    /// `expected.refs` and that no working file to change was edited since `expected.files`
    /// (refused with nothing changed), then moves the refs (deletions, then updates and
    /// `HEAD`, each with the expected old value), the working files, the index and the merge
    /// files.
    pub fn set_state(&self, target: &StateTarget, expected: &StateExpected) -> Result<StateChange> {
        let whole = self.whole()?;
        let now = whole.ref_state()?;
        if let Some(want) = &expected.refs
            && want != &now
        {
            let mut names: Vec<String> = now
                .refs
                .keys()
                .chain(want.refs.keys())
                .filter(|n| now.refs.get(*n) != want.refs.get(*n))
                .cloned()
                .collect();
            if now.head != want.head {
                names.insert(0, "HEAD".into());
            }
            names.dedup();
            return Err(fail(format!(
                "The repository changed since ({}), so nothing was changed.",
                names.join(", ")
            )));
        }
        if target.refs.is_some() && expected.refs.is_none() {
            return Err(fail("setting refs needs their expected values"));
        }
        let from_id = oid(&expected.files)?;
        let from = flatten(&whole.repo, from_id)?;
        let to_id = whole.with_kept(&from, oid(&target.files)?, &target.keep)?;
        let to = flatten(&whole.repo, to_id)?;
        let old = whole.load_index()?;
        let stuck = blocked(&whole, &from, &to, &old)?;
        if !stuck.is_empty() {
            return Err(blocked_error(&whole, "restore", &stuck));
        }
        let index_tree = oid(&target.index)?;
        let conflicts = target.conflicts.as_deref().map(oid).transpose()?;
        let index_changed = whole.index_differs(&old, index_tree, conflicts)?;
        let merge_changed = whole.merge_differs(target.merge.as_deref())?;
        let files = whole.file_changes(&from, &to)?;

        let (mut refs, mut head, mut missing) = (Vec::new(), None, Vec::new());
        if let Some(t) = &target.refs {
            let (deletes, updates, moves, h, m) = whole.ref_edits(&now, t)?;
            if !deletes.is_empty() {
                whole
                    .repo
                    .edit_references(deletes)
                    .ctx("cannot delete a ref")?;
            }
            if !updates.is_empty() {
                whole
                    .repo
                    .edit_references(updates)
                    .ctx("cannot update a ref")?;
            }
            (refs, head, missing) = (moves, h, m);
        }
        let moved = switch(&whole, from_id, to_id, "restore")?;
        let mut index = whole.reindex(index_tree, &[&moved, &old])?;
        if let Some(c) = conflicts {
            let stages: Vec<_> = whole
                .target_entries(index_tree, Some(c))?
                .into_iter()
                .filter(|e| e.1 != 0)
                .collect();
            let paths: BTreeSet<BString> = stages.iter().map(|e| e.0.clone()).collect();
            index.remove_entries(|_, p, _| paths.contains(p));
            for (path, stage, mode, id) in &stages {
                let stage = match stage {
                    1 => Stage::Base,
                    2 => Stage::Ours,
                    _ => Stage::Theirs,
                };
                index.dangerously_push_entry(
                    Default::default(),
                    *id,
                    Flags::from_stage(stage),
                    *mode,
                    path.as_bstr(),
                );
            }
        }
        whole.write_index(&mut index)?;

        whole.clear_merge_state()?;
        if let Some(m) = &target.merge {
            for (name, bytes) in whole.merge_files_of(oid(m)?)? {
                std::fs::write(whole.git_dir().join(&name), bytes)
                    .map_err(|e| fail(format!("cannot write {name}: {e}")))?;
            }
        }
        Ok(StateChange {
            refs,
            head,
            missing,
            files,
            index: index_changed,
            merge: merge_changed,
        })
    }
}

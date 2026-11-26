// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Merging the upstream (pull), and resolving conflicts afterwards.
//!
//! Wave 12 (strand B): the pull merges files on their content model. Git's tree merge
//! (gitoxide) merges the repository; then the repository's file merger (seam N1,
//! [`tk_git_types::FileMerger`], none by default) merges each of its groups and its result
//! replaces Git's, except files that need a decision as a whole
//! (text conflicts), where Git's own result stays. Without a merger Git's merge decides. A file
//! with field conflicts gets the conflict stages 1–3 in the index and, in the working tree,
//! the merged file with ours in each conflicted field and a comment above it. Nothing about
//! the conflicts is stored: a product recomputes them from the index stages
//! ([`Repo::conflict_texts`]), so a merge started on the command line shows the same, and
//! writes the files with the user's choices ([`Repo::write_resolved`]).

use std::collections::{BTreeMap, BTreeSet};

use gix::ObjectId;
use gix::bstr::{BStr, BString, ByteSlice};
use gix::index::entry::{Flags, Mode, Stage};
use gix::merge::tree::{TreatAsUnresolved, apply_index_entries::RemovalMode};
use gix::object::tree::EntryKind;
use tk_git_types::actions::GitMergeFile;
use tk_git_types::{FileMerger, GroupFiles, GroupMerge};
use tk_git_types::{GitConflict, GitIdentity, GitPullResult, GitResolution};

use crate::commit::signature;
use crate::repo::Repo;
use crate::util::{Blob, Context, Flat, Result, blob_bytes, blob_text, fail, flatten};
use crate::worktree::{Probe, stat_of, switch};

/// What is merged into `HEAD` (wave 14 generalises the pull's merge to these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeKind {
    /// Another branch, tag or commit; the base is their merge base, the result has two parents.
    Merge,
    /// The change of one commit (base: its parent) as a new commit with one parent.
    CherryPick,
    /// The change of one commit taken back (base: the commit, theirs: its parent).
    Revert,
}

/// A three-way merge onto `HEAD`.
pub(crate) struct ThreeWay<'a> {
    pub kind: MergeKind,
    /// Pick and revert: the base commit (`None`: the empty tree). Ignored for `Merge`, which
    /// finds the merge base itself.
    pub base: Option<ObjectId>,
    pub ours: ObjectId,
    pub theirs: ObjectId,
    /// Name of their side in conflict markers and the merge message.
    pub label: &'a str,
}

/// One group of the repository merged by the file merger.
struct Semantic {
    /// Repository path of the group's folder, `""` at the root.
    prefix: BString,
    merged: GroupMerge,
    base: GroupFiles,
    theirs: GroupFiles,
}

impl Semantic {
    fn repo_path(&self, rel: &str) -> BString {
        if self.prefix.is_empty() {
            rel.into()
        } else {
            let mut p = self.prefix.clone();
            p.push(b'/');
            p.extend_from_slice(rel.as_bytes());
            p
        }
    }

    /// Group-relative form of a repository path in this group.
    fn rel(&self, path: &BStr) -> Option<String> {
        if self.prefix.is_empty() {
            return path.to_str().ok().map(str::to_owned);
        }
        let rest = path.strip_prefix(self.prefix.as_slice())?;
        let rest = rest.strip_prefix(b"/")?;
        rest.to_str().ok().map(str::to_owned)
    }
}

/// The merger's groups on any of `flats` (repository paths of their folders).
fn groups(merger: &dyn FileMerger, flats: &[&Flat]) -> BTreeSet<BString> {
    let paths: BTreeSet<&str> = flats
        .iter()
        .flat_map(|flat| flat.keys())
        .filter_map(|p| p.to_str().ok())
        .collect();
    let paths: Vec<&str> = paths.into_iter().collect();
    merger
        .groups(&paths)
        .into_iter()
        .map(BString::from)
        .collect()
}

/// `path` relative to the group folder `prefix` (`""`: the root), if it is inside.
fn rel_to<'p>(prefix: &BStr, path: &'p BStr) -> Option<&'p BStr> {
    if prefix.is_empty() {
        return Some(path);
    }
    path.strip_prefix(prefix.as_bytes())
        .and_then(|r| r.strip_prefix(b"/"))
        .map(|r| r.as_bstr())
}

/// The files of the group at `prefix` in `flat` that the merger claims (regular files only).
fn files_under(
    repo: &gix::Repository,
    merger: &dyn FileMerger,
    flat: &Flat,
    prefix: &BStr,
) -> Result<GroupFiles> {
    let mut files = GroupFiles::new();
    for (path, blob) in flat {
        if !matches!(blob.mode, Mode::FILE | Mode::FILE_EXECUTABLE) {
            continue;
        }
        let Some(rel) = rel_to(prefix, path.as_bstr()).and_then(|r| r.to_str().ok()) else {
            continue;
        };
        if merger.claims(rel) {
            files.insert(rel.to_owned(), blob_bytes(repo, blob.id)?);
        }
    }
    Ok(files)
}

/// Files under `prefix` that differ between `a` and `b`.
fn differs_under(a: &Flat, b: &Flat, prefix: &BStr) -> bool {
    let inside = |p: &BString| {
        prefix.is_empty()
            || p.strip_prefix(prefix.as_bytes())
                .is_some_and(|r| r.starts_with(b"/"))
    };
    a.iter()
        .filter(|(p, _)| inside(p))
        .any(|(p, blob)| b.get(p) != Some(blob))
        || b.iter()
            .filter(|(p, _)| inside(p))
            .any(|(p, _)| !a.contains_key(p))
}

/// A merge computed but not applied: the merged tree (in the object database of the
/// repository it was computed on, possibly in memory) and what conflicts.
pub(crate) struct Computed<'r> {
    outcome: gix::merge::tree::Outcome<'r>,
    base: Option<ObjectId>,
    sems: Vec<Semantic>,
    /// Group files replaced by the model's merge: path, blob (`None`: deleted), whether
    /// it has field conflicts.
    overlay: Vec<(BString, Option<ObjectId>, bool)>,
    flats: [Flat; 3],
}

/// Line ranges of conflict markers in a merged text (`lines 3-9`).
fn marker_ranges(text: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = None;
    for (n, line) in text.split(|b| *b == b'\n').enumerate() {
        if line.starts_with(b"<<<<<<<") {
            start = Some(n + 1);
        } else if line.starts_with(b">>>>>>>")
            && let Some(s) = start.take()
        {
            out.push(format!("lines {s}-{}", n + 1));
        }
    }
    out
}

impl Repo {
    /// Merges `theirs` into `ours` (the current branch, at `HEAD`). Without conflicts: a merge
    /// commit. With conflicts: the merge stops (`MERGE_HEAD`, conflict stages in the index,
    /// conflict markers in the files).
    pub(crate) fn merge(
        &self,
        ours: ObjectId,
        theirs: ObjectId,
        their_label: &str,
        identity: &GitIdentity,
    ) -> Result<GitPullResult> {
        let branch = self.head_branch()?.unwrap_or_else(|| "HEAD".into());
        let tw = ThreeWay {
            kind: MergeKind::Merge,
            base: None,
            ours,
            theirs,
            label: their_label,
        };
        let computed = self.compute_merge(&self.repo, &tw)?;
        let message = format!("Merge {their_label} into {branch}");
        self.apply_merge(computed, &tw, identity, None, &message, "pull")
    }

    /// Computes the three-way merge `tw` with objects written to `repo` (this repository, or
    /// a copy of it with objects in memory for a preview). Git's tree merge merges every
    /// file; then each group of the file merger is merged on the model and its files replace
    /// Git's result, except files that need a decision as a whole
    /// (text conflicts), where Git's own result stays.
    pub(crate) fn compute_merge<'r>(
        &self,
        repo: &'r gix::Repository,
        tw: &ThreeWay<'_>,
    ) -> Result<Computed<'r>> {
        let branch = self.head_branch()?.unwrap_or_else(|| "HEAD".into());
        let labels = gix::merge::blob::builtin_driver::text::Labels {
            ancestor: Some(BStr::new("base")),
            current: Some(BStr::new(branch.as_str())),
            other: Some(BStr::new(tw.label)),
        };
        // No rename tracking: a conflict is always about one path, the way the UI shows it.
        let options = repo
            .tree_merge_options()
            .ctx("cannot read the merge configuration")?
            .with_rewrites(None);
        let fail_merge = |e: gix::Error| fail(format!("cannot merge {}: {e}", tw.label));
        let (mut outcome, base) = match tw.kind {
            MergeKind::Merge => {
                let o = repo
                    .merge_commits(tw.ours, tw.theirs, labels, options.into())
                    .map_err(|e| fail_merge(gix::Error::from_error(e)))?;
                let base = repo
                    .merge_base(tw.ours, tw.theirs)
                    .ok()
                    .flatten()
                    .map(|id| id.detach());
                (o.tree_merge, base)
            }
            MergeKind::CherryPick | MergeKind::Revert => {
                let base_tree = match tw.base {
                    Some(c) => self.commit_tree(c)?,
                    None => ObjectId::empty_tree(repo.object_hash()),
                };
                let o = repo
                    .merge_trees(
                        base_tree,
                        self.commit_tree(tw.ours)?,
                        self.commit_tree(tw.theirs)?,
                        labels,
                        options,
                    )
                    .map_err(|e| fail_merge(gix::Error::from_error(e)))?;
                (o, tw.base)
            }
        };
        let flat = |c: Option<ObjectId>| -> Result<Flat> {
            match c {
                Some(c) => flatten(&self.repo, self.commit_tree(c)?),
                None => Ok(Flat::new()),
            }
        };
        let flats = [flat(base)?, flat(Some(tw.ours))?, flat(Some(tw.theirs))?];
        let sems = self.semantics(&flats)?;
        let mut overlay: Vec<(BString, Option<ObjectId>, bool)> = Vec::new();
        for sem in &sems {
            let m = &sem.merged;
            for p in &m.paths {
                if m.whole_file.contains(p) {
                    continue;
                }
                let repo_path = sem.repo_path(p);
                let fields = m.conflicts.contains_key(p);
                let content = match fields {
                    true => m.marked.get(p),
                    false => None,
                }
                .or_else(|| m.files.get(p));
                let id = match content {
                    Some(bytes) => {
                        let id = repo
                            .write_blob(bytes)
                            .ctx("cannot write a merged file")?
                            .detach();
                        outcome
                            .tree
                            .upsert(repo_path.clone(), EntryKind::Blob, id)
                            .ctx("cannot write the merged tree")?;
                        Some(id)
                    }
                    None => {
                        outcome
                            .tree
                            .remove(repo_path.clone())
                            .ctx("cannot write the merged tree")?;
                        None
                    }
                };
                overlay.push((repo_path, id, fields));
            }
        }
        Ok(Computed {
            outcome,
            base,
            sems,
            overlay,
            flats,
        })
    }

    /// Every group of the repository merged by the file merger; a group the merger cannot
    /// merge is left to Git's merge, and so is everything without a merger.
    fn semantics(&self, flats: &[Flat; 3]) -> Result<Vec<Semantic>> {
        let Some(merger) = self.merger.as_deref() else {
            return Ok(Vec::new());
        };
        let [b, o, t] = flats;
        let mut out = Vec::new();
        for prefix in groups(merger, &[b, o, t]) {
            let prefix_ref = prefix.as_bstr();
            // Nothing comes in, or both sides agree: Git's result is ours already.
            if !differs_under(b, t, prefix_ref) || !differs_under(o, t, prefix_ref) {
                continue;
            }
            let (bf, of, tf) = (
                files_under(&self.repo, merger, b, prefix_ref)?,
                files_under(&self.repo, merger, o, prefix_ref)?,
                files_under(&self.repo, merger, t, prefix_ref)?,
            );
            let Some(merged) = merger.merge(&bf, &of, &tf) else {
                continue;
            };
            out.push(Semantic {
                prefix,
                merged,
                base: bf,
                theirs: tf,
            });
        }
        Ok(out)
    }

    /// Whether the file merger decides the repository path `path` on `flats`.
    pub(crate) fn merges_on_model(&self, flats: &[&Flat], path: &BStr) -> bool {
        let Some(merger) = self.merger.as_deref() else {
            return false;
        };
        groups(merger, flats).iter().any(|root| {
            rel_to(root.as_bstr(), path)
                .and_then(|r| r.to_str().ok())
                .is_some_and(|r| merger.claims(r))
        })
    }

    /// The files a merge changes on our side, for a plan: semantic per file the merger
    /// decides (the parts that merge and those that conflict), line by line for every other.
    pub(crate) fn merge_preview(&self, mut c: Computed<'_>) -> Result<Vec<GitMergeFile>> {
        let how = TreatAsUnresolved::git();
        let mut unresolved: BTreeSet<BString> = BTreeSet::new();
        for conflict in &c.outcome.conflicts {
            if conflict.is_unresolved(how) {
                unresolved.insert(conflict.ours.location().to_owned());
                unresolved.insert(conflict.theirs.location().to_owned());
            }
        }
        let merged = c.outcome.tree.write().ctx("cannot write the merged tree")?;
        let merged_repo = merged.repo;
        let merged = flatten(merged_repo, merged.detach())?;
        let [b, o, t] = &c.flats;
        let mut paths: BTreeSet<&BString> = BTreeSet::new();
        for p in b.keys().chain(o.keys()).chain(t.keys()) {
            // Their change, and the merge changes our file (or needs a decision: Git's
            // conflicts count only where the model did not merge the file).
            let modelled = c.sems.iter().any(|s| {
                s.rel(p.as_bstr()).is_some_and(|r| {
                    s.merged.paths.contains(&r) && !s.merged.whole_file.contains(&r)
                })
            });
            if b.get(p) != t.get(p)
                && o.get(p) != t.get(p)
                && (merged.get(p) != o.get(p) || (!modelled && unresolved.contains(p)))
            {
                paths.insert(p);
            }
        }
        // Changed parts per file of each group, base → theirs.
        let mut fields_in: BTreeMap<BString, u32> = BTreeMap::new();
        if let Some(merger) = self.merger.as_deref() {
            for sem in &c.sems {
                for (file, n) in merger.changed_parts(&sem.base, &sem.theirs) {
                    *fields_in.entry(sem.repo_path(&file)).or_default() += n;
                }
            }
        }
        let text = |flat: &Flat, p: &BString| -> Result<Option<Vec<u8>>> {
            flat.get(p)
                .map(|blob| blob_bytes(&self.repo, blob.id))
                .transpose()
        };
        let mut out = Vec::new();
        for p in paths {
            let sem = c.sems.iter().find_map(|s| {
                s.rel(p.as_bstr())
                    .filter(|r| s.merged.paths.contains(r))
                    .map(|r| (s, r))
            });
            let file = match sem {
                Some((s, rel)) if !s.merged.whole_file.contains(&rel) => {
                    let conflicts = s.merged.conflicts.get(&rel).cloned().unwrap_or_default();
                    let changed = fields_in.get(p).copied().unwrap_or(1).max(1);
                    let n = u32::try_from(conflicts.len()).unwrap_or(u32::MAX);
                    GitMergeFile {
                        path: format!("/{}", p.to_str_lossy()),
                        semantic: true,
                        clean: if conflicts.is_empty() {
                            changed
                        } else {
                            changed.saturating_sub(n)
                        },
                        conflicts,
                    }
                }
                _ => {
                    let hunks = tk_git_types::text_diff(
                        &p.to_str_lossy(),
                        text(b, p)?.as_deref(),
                        text(t, p)?.as_deref(),
                    )
                    .hunks
                    .len();
                    let hunks = u32::try_from(hunks.max(1)).unwrap_or(u32::MAX);
                    let conflicts = if unresolved.contains(p) {
                        let ranges = match merged.get(p) {
                            Some(blob) => marker_ranges(&blob_bytes(merged_repo, blob.id)?),
                            None => Vec::new(),
                        };
                        if ranges.is_empty() {
                            vec!["whole file".to_owned()]
                        } else {
                            ranges
                        }
                    } else {
                        Vec::new()
                    };
                    let n = u32::try_from(conflicts.len()).unwrap_or(u32::MAX);
                    GitMergeFile {
                        path: format!("/{}", p.to_str_lossy()),
                        semantic: false,
                        clean: if conflicts.is_empty() {
                            hunks
                        } else {
                            hunks.saturating_sub(n)
                        },
                        conflicts,
                    }
                }
            };
            out.push(file);
        }
        Ok(out)
    }

    /// Applies a computed merge to `HEAD` and the working tree: a commit (a merge commit for
    /// [`MergeKind::Merge`], else one parent) when it is clean; otherwise the merge stops like
    /// `git merge`/`git cherry-pick`: `MERGE_HEAD` (or `CHERRY_PICK_HEAD`, `REVERT_HEAD`) and
    /// `MERGE_MSG`, the conflict stages in the index, markers (or field comments) in the files.
    /// `author`: kept from the picked commit; `None`: `identity`.
    pub(crate) fn apply_merge(
        &self,
        mut c: Computed<'_>,
        tw: &ThreeWay<'_>,
        identity: &GitIdentity,
        author: Option<gix::actor::Signature>,
        message: &str,
        what: &str,
    ) -> Result<GitPullResult> {
        let how = TreatAsUnresolved::git();
        let tree = c
            .outcome
            .tree
            .write()
            .ctx("cannot write the merged tree")?
            .detach();
        let mut index = switch(self, self.commit_tree(tw.ours)?, tree, what)?;
        if c.outcome.has_unresolved_conflicts(how) {
            c.outcome
                .index_changed_after_applying_conflicts(&mut index, how, RemovalMode::Prune);
        }
        for (repo_path, id, fields) in &c.overlay {
            index.remove_entries(|_, p, _| p == repo_path);
            if *fields {
                for (commit, stage) in [
                    (c.base, Stage::Base),
                    (Some(tw.ours), Stage::Ours),
                    (Some(tw.theirs), Stage::Theirs),
                ] {
                    let Some(commit) = commit else { continue };
                    if let Some((mode, blob)) = self.entry_at(commit, repo_path)? {
                        index.dangerously_push_entry(
                            Default::default(),
                            blob,
                            Flags::from_stage(stage),
                            mode,
                            repo_path.as_ref(),
                        );
                    }
                }
            } else if let Some(id) = id {
                let file = crate::worktree::fs_path(&self.workdir, repo_path.as_ref())?;
                index.dangerously_push_entry(
                    stat_of(&file),
                    *id,
                    Flags::empty(),
                    Mode::FILE,
                    repo_path.as_ref(),
                );
            }
        }
        let conflicted = index
            .entries()
            .iter()
            .any(|e| e.stage() != Stage::Unconflicted);
        if !conflicted {
            let committer = signature(identity);
            let author = author.unwrap_or_else(|| committer.clone());
            let parents: Vec<ObjectId> = match tw.kind {
                MergeKind::Merge => vec![tw.ours, tw.theirs],
                MergeKind::CherryPick | MergeKind::Revert => vec![tw.ours],
            };
            let id = self.commit_on("HEAD", &committer, &author, message, tree, parents)?;
            self.write_index(&mut index)?;
            return Ok(GitPullResult::Merged {
                oid: id.to_string(),
            });
        }
        self.write_index(&mut index)?;
        let git_dir = self.git_dir();
        let io = |e: std::io::Error| fail(format!("cannot record the merge: {e}"));
        let head_file = match tw.kind {
            MergeKind::Merge => "MERGE_HEAD",
            MergeKind::CherryPick => "CHERRY_PICK_HEAD",
            MergeKind::Revert => "REVERT_HEAD",
        };
        // REVERT_HEAD names the reverted commit (the base; theirs is its parent).
        let named = match tw.kind {
            MergeKind::Revert => tw.base.unwrap_or(tw.theirs),
            _ => tw.theirs,
        };
        std::fs::write(git_dir.join(head_file), format!("{named}\n")).map_err(io)?;
        std::fs::write(git_dir.join("MERGE_MSG"), format!("{message}\n")).map_err(io)?;
        if tw.kind == MergeKind::Merge {
            std::fs::write(git_dir.join("MERGE_MODE"), "").map_err(io)?;
        }
        let paths: BTreeSet<String> = index
            .entries()
            .iter()
            .filter(|e| e.stage() != Stage::Unconflicted)
            .map(|e| self.api_path(e.path(&index)))
            .collect();
        Ok(GitPullResult::Conflicts {
            paths: paths.into_iter().collect(),
        })
    }

    /// The conflicted files of the scope with the text of their index stages (base, ours,
    /// theirs; `None` where a side lacks the file), sorted by path: what a product recomputes
    /// conflicts on its content model from.
    pub fn conflict_texts(&self) -> Result<Vec<GitConflict>> {
        let index = self.load_index()?;
        let mut out: BTreeMap<String, GitConflict> = BTreeMap::new();
        for e in index.entries() {
            let stage = e.stage();
            if stage == Stage::Unconflicted {
                continue;
            }
            let Some(rel) = self.scope.rel(e.path(&index)) else {
                continue;
            };
            let text = blob_text(&self.repo, e.id)?;
            let entry = out.entry(rel.clone()).or_insert_with(|| GitConflict {
                path: rel,
                base: None,
                ours: None,
                theirs: None,
            });
            match stage {
                Stage::Base => entry.base = Some(text),
                Stage::Ours => entry.ours = Some(text),
                Stage::Theirs => entry.theirs = Some(text),
                Stage::Unconflicted => {}
            }
        }
        Ok(out.into_values().collect())
    }

    /// A merge, cherry-pick or revert stopped on conflicts and is not concluded yet.
    pub fn in_progress(&self) -> Result<bool> {
        Ok(self.underway()?.is_some())
    }

    /// Writes resolved files of the merge in progress (scope-relative path, content; `None`
    /// deletes the file) to the working tree and marks them resolved in the index. The next
    /// [`Repo::commit`] concludes the merge. Refused when no merge is in progress.
    pub fn write_resolved(&self, files: &[(String, Option<Vec<u8>>)]) -> Result<()> {
        if self.underway()?.is_none() {
            return Err(fail("no merge is in progress"));
        }
        let mut index = self.load_index()?;
        let mut probe = Probe::new(self)?;
        for (path, content) in files {
            let repo_path = self.scope.abs(path)?;
            index.remove_entries(|_, p, _| p == repo_path);
            match content {
                Some(bytes) => {
                    let id = self
                        .repo
                        .write_blob(bytes)
                        .ctx("cannot write a resolved file")?
                        .detach();
                    let blob = Blob {
                        mode: Mode::FILE,
                        id,
                    };
                    let file = probe.write(repo_path.as_ref(), blob)?;
                    index.dangerously_push_entry(
                        stat_of(&file),
                        id,
                        Flags::empty(),
                        Mode::FILE,
                        repo_path.as_ref(),
                    );
                }
                None => {
                    let file = crate::worktree::fs_path(&self.workdir, repo_path.as_ref())?;
                    match std::fs::remove_file(&file) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(fail(format!("cannot delete {path}: {e}"))),
                    }
                }
            }
        }
        self.write_index(&mut index)?;
        Ok(())
    }

    fn stage_blob(&self, index: &gix::index::State, path: &BStr, stage: Stage) -> Option<Blob> {
        index.entry_by_path_and_stage(path, stage).map(|e| Blob {
            mode: e.mode,
            id: e.id,
        })
    }

    /// Base, ours and theirs of a conflicted file.
    pub fn conflict(&self, path: &str) -> Result<GitConflict> {
        let repo_path = self.repo_path(path)?;
        let index = self.load_index()?;
        let side = |stage| self.stage_blob(&index, repo_path.as_ref(), stage);
        let (base, ours, theirs) = (side(Stage::Base), side(Stage::Ours), side(Stage::Theirs));
        if base.is_none() && ours.is_none() && theirs.is_none() {
            return Err(fail(format!("{path} has no conflict")));
        }
        let text = |b: Option<Blob>| b.map(|b| blob_text(&self.repo, b.id)).transpose();
        Ok(GitConflict {
            path: path.to_owned(),
            base: text(base)?,
            ours: text(ours)?,
            theirs: text(theirs)?,
        })
    }

    /// Resolves a conflicted file with one side: writes it to the working tree (or deletes
    /// the file if that side deleted it) and marks it resolved in the index.
    pub fn resolve(&self, path: &str, resolution: GitResolution) -> Result<()> {
        let repo_path = self.repo_path(path)?;
        let mut index = self.load_index()?;
        let conflicted = index
            .entries()
            .iter()
            .any(|e| e.stage() != Stage::Unconflicted && e.path(&index) == repo_path);
        if !conflicted {
            return Err(fail(format!("{path} has no conflict")));
        }
        let stage = match resolution {
            GitResolution::Ours => Stage::Ours,
            GitResolution::Theirs => Stage::Theirs,
        };
        let chosen = self.stage_blob(&index, repo_path.as_ref(), stage);
        index.remove_entries(|_, p, _| p == repo_path);
        match chosen {
            Some(blob) => {
                let file = Probe::new(self)?.write(repo_path.as_ref(), blob)?;
                index.dangerously_push_entry(
                    stat_of(&file),
                    blob.id,
                    Flags::empty(),
                    blob.mode,
                    repo_path.as_ref(),
                );
            }
            None => {
                let file = crate::worktree::fs_path(&self.workdir, repo_path.as_ref())?;
                match std::fs::remove_file(&file) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(fail(format!("cannot delete {path}: {e}"))),
                }
            }
        }
        self.write_index(&mut index)
    }
}

impl Repo {
    /// The merged tree of a computed merge (objects written to the repository it was computed
    /// on) for `GitApi.pick`: every conflicted path holds our version in it. Also the text
    /// conflicts (base, ours, theirs) and the paths whose conflicts are not text. Wave 15,
    /// strand D.
    pub(crate) fn pick_outcome(
        &self,
        mut c: Computed<'_>,
    ) -> Result<(ObjectId, Vec<GitConflict>, Vec<String>)> {
        let how = TreatAsUnresolved::git();
        let mut conflicted: BTreeSet<BString> = BTreeSet::new();
        for conflict in &c.outcome.conflicts {
            if conflict.is_unresolved(how) {
                conflicted.insert(conflict.ours.location().to_owned());
                conflicted.insert(conflict.theirs.location().to_owned());
            }
        }
        // Files merged on the model are decided by it, not by Git's line merge.
        for (path, _, fields) in &c.overlay {
            if *fields {
                conflicted.insert(path.clone());
            } else {
                conflicted.remove(path);
            }
        }
        let tree = c
            .outcome
            .tree
            .write()
            .ctx("cannot write the merged tree")?
            .detach();
        let [base, ours, theirs] = &c.flats;
        let tree = if conflicted.is_empty() {
            tree
        } else {
            let mut editor = self.repo.edit_tree(tree).ctx("cannot build the tree")?;
            for p in &conflicted {
                match ours.get(p) {
                    Some(blob) => editor
                        .upsert(p.as_bstr(), crate::commit::entry_kind(blob.mode), blob.id)
                        .ctx("cannot build the tree")?,
                    None => editor.remove(p.as_bstr()).ctx("cannot build the tree")?,
                };
            }
            editor.write().ctx("cannot write the tree")?.detach()
        };
        let mut texts = Vec::new();
        let mut other = Vec::new();
        for p in &conflicted {
            let path = format!("/{}", p.to_str_lossy());
            let side = |flat: &Flat| -> Result<Option<Option<String>>> {
                match flat.get(p) {
                    None => Ok(Some(None)),
                    Some(b) if matches!(b.mode, Mode::FILE | Mode::FILE_EXECUTABLE) => {
                        Ok(String::from_utf8(blob_bytes(&self.repo, b.id)?)
                            .ok()
                            .map(Some))
                    }
                    Some(_) => Ok(None),
                }
            };
            match (side(base)?, side(ours)?, side(theirs)?) {
                (Some(base), Some(ours), Some(theirs)) => texts.push(GitConflict {
                    path,
                    base,
                    ours,
                    theirs,
                }),
                _ => other.push(path),
            }
        }
        Ok((tree, texts, other))
    }
}

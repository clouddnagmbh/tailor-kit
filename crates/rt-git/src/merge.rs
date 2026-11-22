// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Merging the upstream (pull), and resolving conflicts afterwards.
//!
//! Wave 12 (strand B): the pull merges the collection on the model. Git's tree merge
//! (gitoxide) merges the repository; then every collection file is replaced by the result of
//! `rt_format::merge` (the same merge as `rt merge-driver` and the web app), except files
//! that need a decision as a whole (text conflicts), where Git's own result stays. A file
//! with field conflicts gets the conflict stages 1–3 in the index and, in the working tree,
//! the merged file with ours in each conflicted field and a comment above it. Nothing about
//! the conflicts is stored: [`Repo::merge_state`] recomputes them from the index stages, so
//! a merge started on the command line shows the same, and [`Repo::resolve_merge`] writes
//! the files with the user's choices.

use std::collections::{BTreeMap, BTreeSet};

use gix::ObjectId;
use gix::bstr::{BStr, BString};
use gix::index::entry::{Flags, Mode, Stage};
use gix::merge::tree::{TreatAsUnresolved, apply_index_entries::RemovalMode};
use gix::object::tree::EntryKind;
use rt_app::git::{GitConflict, GitIdentity, GitPullResult, GitResolution};
use rt_app::merges::{ConflictChoice, ConflictStages, MergeResult, MergeState};

use crate::commit::signature;
use crate::repo::Repo;
use crate::util::{Blob, Context, Result, blob_text, fail};
use crate::worktree::{Probe, stat_of, switch};

/// The merge of the collection on the model, for the pull.
struct Semantic {
    base: Option<ObjectId>,
    result: MergeResult,
    /// Working-tree text of conflicted files (comments at conflicted fields).
    marked: BTreeMap<String, Vec<u8>>,
    /// Every collection file on any side.
    paths: BTreeSet<String>,
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
        let labels = gix::merge::blob::builtin_driver::text::Labels {
            ancestor: Some(BStr::new("base")),
            current: Some(BStr::new(branch.as_str())),
            other: Some(BStr::new(their_label)),
        };
        // No rename tracking: a conflict is always about one path, the way the UI shows it.
        let options = self
            .repo
            .tree_merge_options()
            .ctx("cannot read the merge configuration")?
            .with_rewrites(None);
        let mut outcome = self
            .repo
            .merge_commits(ours, theirs, labels, options.into())
            .map_err(|e| fail(format!("cannot merge {their_label}: {e}")))?;
        let how = TreatAsUnresolved::git();
        // The collection merges on the model; Git's result stays only for text conflicts.
        let sem = self.semantic(ours, theirs)?;
        let mut overlay: Vec<(BString, Option<ObjectId>, bool)> = Vec::new();
        if let Some(sem) = &sem {
            for p in &sem.paths {
                if sem.result.text_conflicts.contains(p) {
                    continue;
                }
                let repo_path = self.scope.abs(p)?;
                let fields = sem.result.conflicts.iter().any(|c| &c.file == p);
                let content = match fields {
                    true => sem.marked.get(p),
                    false => None,
                }
                .or_else(|| sem.result.files.files.get(p));
                let id = match content {
                    Some(bytes) => {
                        let id = self
                            .repo
                            .write_blob(bytes)
                            .ctx("cannot write a merged file")?
                            .detach();
                        outcome
                            .tree_merge
                            .tree
                            .upsert(repo_path.clone(), EntryKind::Blob, id)
                            .ctx("cannot write the merged tree")?;
                        Some(id)
                    }
                    None => {
                        outcome
                            .tree_merge
                            .tree
                            .remove(repo_path.clone())
                            .ctx("cannot write the merged tree")?;
                        None
                    }
                };
                overlay.push((repo_path, id, fields));
            }
        }
        let tree = outcome
            .tree_merge
            .tree
            .write()
            .ctx("cannot write the merged tree")?
            .detach();
        let message = format!("Merge {their_label} into {branch}");
        let mut index = switch(self, self.commit_tree(ours)?, tree, "pull")?;
        if outcome.tree_merge.has_unresolved_conflicts(how) {
            outcome.tree_merge.index_changed_after_applying_conflicts(
                &mut index,
                how,
                RemovalMode::Prune,
            );
        }
        let base = sem.as_ref().and_then(|s| s.base);
        for (repo_path, id, fields) in &overlay {
            index.remove_entries(|_, p, _| p == repo_path);
            if *fields {
                for (commit, stage) in [
                    (base, Stage::Base),
                    (Some(ours), Stage::Ours),
                    (Some(theirs), Stage::Theirs),
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
            let sig = signature(identity);
            let mut time = gix::date::parse::TimeBuf::default();
            let sig = sig.to_ref(&mut time);
            let id = self
                .repo
                .commit_as(sig, sig, "HEAD", &message, tree, [ours, theirs])
                .ctx("cannot commit the merge")?
                .detach();
            self.write_index(&mut index)?;
            return Ok(GitPullResult::Merged {
                oid: id.to_string(),
            });
        }
        self.write_index(&mut index)?;
        let git_dir = self.git_dir();
        let io = |e: std::io::Error| fail(format!("cannot record the merge: {e}"));
        std::fs::write(git_dir.join("MERGE_HEAD"), format!("{theirs}\n")).map_err(io)?;
        std::fs::write(git_dir.join("MERGE_MSG"), format!("{message}\n")).map_err(io)?;
        std::fs::write(git_dir.join("MERGE_MODE"), "").map_err(io)?;
        let paths: BTreeSet<String> = index
            .entries()
            .iter()
            .filter(|e| e.stage() != Stage::Unconflicted)
            .filter_map(|e| self.scope.rel(e.path(&index)))
            .collect();
        Ok(GitPullResult::Conflicts {
            paths: paths.into_iter().collect(),
        })
    }

    /// The collection's merge on the model, or `None` when the collection cannot be read on
    /// some side (then Git's merge is all there is).
    fn semantic(&self, ours: ObjectId, theirs: ObjectId) -> Result<Option<Semantic>> {
        let base = self
            .repo
            .merge_base(ours, theirs)
            .ok()
            .flatten()
            .map(|id| id.detach());
        let files = |id: Option<ObjectId>| -> Result<_> {
            Ok(match id {
                Some(id) => self.tree_at(&id.to_string())?.to_file_set(),
                None => Default::default(),
            })
        };
        let (b, o, t) = (files(base)?, files(Some(ours))?, files(Some(theirs))?);
        let Ok((result, marked)) = rt_app::merges::pull_merge(&b, &o, &t) else {
            return Ok(None);
        };
        let paths = b
            .files
            .keys()
            .chain(o.files.keys())
            .chain(t.files.keys())
            .cloned()
            .collect();
        Ok(Some(Semantic {
            base,
            result,
            marked,
            paths,
        }))
    }

    /// The conflicted files of the collection with their index stages.
    fn conflict_stages(&self) -> Result<Vec<ConflictStages>> {
        let index = self.load_index()?;
        let mut out: BTreeMap<String, ConflictStages> = BTreeMap::new();
        for e in index.entries() {
            let stage = e.stage();
            if stage == Stage::Unconflicted {
                continue;
            }
            let Some(rel) = self.scope.rel(e.path(&index)) else {
                continue;
            };
            let text = blob_text(&self.repo, e.id)?;
            let entry = out.entry(rel.clone()).or_insert_with(|| ConflictStages {
                path: rel,
                ..Default::default()
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

    /// The collection at `HEAD`, the context for recomputing conflicts.
    fn head_files(&self) -> Result<rt_app::merges::FileSet> {
        Ok(match self.head_commit()? {
            Some(id) => self.tree_at(&id.to_string())?.to_file_set(),
            None => Default::default(),
        })
    }

    /// The conflicts of the merge in progress per field, recomputed from the index stages
    /// (`MergeApi.conflicts`).
    pub fn merge_state(&self) -> Result<MergeState> {
        let merging = self.merge_head()?.is_some();
        let stages = self.conflict_stages()?;
        if stages.is_empty() {
            return Ok(MergeState {
                merging,
                ..MergeState::default()
            });
        }
        rt_app::merges::state(&self.head_files()?, &stages, merging)
    }

    /// Resolves the merge in progress with `choices` (`MergeApi.resolve`): every conflicted
    /// file whose conflicts the choices all decide (or that merges cleanly on the model) is
    /// written and marked resolved; the others stay. Returns the paths written. The next
    /// [`Repo::commit`] concludes the merge.
    pub fn resolve_merge(&self, choices: &[ConflictChoice]) -> Result<Vec<String>> {
        if self.merge_head()?.is_none() {
            return Err(fail("no merge is in progress"));
        }
        let stages = self.conflict_stages()?;
        let files = rt_app::merges::resolution(&self.head_files()?, &stages, choices)?;
        let mut index = self.load_index()?;
        let mut probe = Probe::new(self)?;
        for (path, content) in &files {
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
        Ok(files.into_iter().map(|(p, _)| p).collect())
    }

    fn stage_blob(&self, index: &gix::index::State, path: &BStr, stage: Stage) -> Option<Blob> {
        index.entry_by_path_and_stage(path, stage).map(|e| Blob {
            mode: e.mode,
            id: e.id,
        })
    }

    /// Base, ours and theirs of a conflicted file.
    pub fn conflict(&self, path: &str) -> Result<GitConflict> {
        let repo_path = self.scope.abs(path)?;
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
        let repo_path = self.scope.abs(path)?;
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

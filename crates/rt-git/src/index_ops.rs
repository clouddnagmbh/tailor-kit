// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The index as building blocks (wave 15, strand F; types in `rt_app::git::index`): diff of
//! each side, stage and unstage whole files or chosen lines, discard chosen lines from the
//! working tree, commit what is staged.
//!
//! The app's own commit stays "commit without staging" (`Repo::commit`); nothing here runs
//! unless an extension's staging view calls it. Results equal `git add -p`, `git reset -p`
//! and `git checkout -p` on the same selection (the tests compare with the command line).
//!
//! Lines are chosen on the diff of the raw bytes (no `eol` or clean filters), as Git shows
//! them; a selection carries the blob ids of both sides and is refused when either changed.

use gix::ObjectId;
use gix::bstr::{BStr, BString, ByteSlice};
use gix::index::entry::{Flags, Mode, Stage, Stat};
use rt_app::AppError;
use rt_app::git::index::{GitIndexDiff, GitPatchRequest, GitPatchSide, GitStagedCommit};
use rt_app::git::{GitCommit, text_diff};
use rt_app::secret_scan;

use crate::commit::{describe, signature};
use crate::patch::Selection;
use crate::repo::Repo;
use crate::util::{Context, Result, blob_bytes, fail};
use crate::worktree::{fs_path, stat_of};

/// One side of a file: its blob and what it holds.
struct Side {
    id: Option<ObjectId>,
    mode: Option<Mode>,
    bytes: Option<Vec<u8>>,
}

impl Side {
    fn none() -> Self {
        Self {
            id: None,
            mode: None,
            bytes: None,
        }
    }

    fn id_text(&self) -> String {
        self.id.map(|i| i.to_string()).unwrap_or_default()
    }
}

fn text_of(side: &Side, shown: &str) -> Result<String> {
    match &side.bytes {
        None => Ok(String::new()),
        Some(b) if b.contains(&0) => Err(fail(format!(
            "{shown} is not a text file: choose it as a whole"
        ))),
        Some(b) => String::from_utf8(b.clone())
            .map_err(|_| fail(format!("{shown} is not UTF-8 text: choose it as a whole"))),
    }
}

impl Repo {
    fn index_path_of(&self, path: &str) -> Result<BString> {
        self.repo_path(path)
    }

    /// Refuses a path with conflict stages (resolve it first).
    fn refuse_conflict(index: &gix::index::State, path: &BStr, shown: &str) -> Result<()> {
        if index
            .entries()
            .iter()
            .any(|e| e.stage() != Stage::Unconflicted && e.path(index) == path)
        {
            return Err(fail(format!("{shown} has a conflict: resolve it first")));
        }
        Ok(())
    }

    fn index_side(&self, index: &gix::index::State, path: &BStr) -> Result<Side> {
        match index.entry_by_path(path) {
            Some(e) => Ok(Side {
                id: Some(e.id),
                mode: Some(e.mode),
                bytes: Some(blob_bytes(&self.repo, e.id)?),
            }),
            None => Ok(Side::none()),
        }
    }

    fn head_side(&self, path: &BStr) -> Result<Side> {
        let tree = self.head_tree()?;
        let tree = self.repo.find_tree(tree).ctx("cannot read the HEAD tree")?;
        let Some(entry) = tree
            .lookup_entry_by_path(gix::path::from_bstr(path).ctx("invalid path")?.as_ref())
            .ctx("cannot read the HEAD tree")?
        else {
            return Ok(Side::none());
        };
        let kind = entry.mode().kind();
        if matches!(
            kind,
            gix::objs::tree::EntryKind::Tree | gix::objs::tree::EntryKind::Commit
        ) {
            return Ok(Side::none());
        }
        let id = entry.object_id();
        Ok(Side {
            id: Some(id),
            mode: Some(crate::commit::index_mode(kind)),
            bytes: Some(blob_bytes(&self.repo, id)?),
        })
    }

    /// The working file as it is (raw bytes), `None` if it is missing.
    fn work_side(&self, path: &BStr, shown: &str) -> Result<Side> {
        let file = fs_path(&self.workdir, path)?;
        match std::fs::symlink_metadata(&file) {
            Ok(md) if md.is_file() => {
                let bytes = std::fs::read(&file).ctx("cannot read a file")?;
                let id =
                    gix::objs::compute_hash(self.repo.object_hash(), gix::objs::Kind::Blob, &bytes)
                        .ctx("cannot hash a file")?;
                let mode = if gix::fs::is_executable(&md) {
                    Mode::FILE_EXECUTABLE
                } else {
                    Mode::FILE
                };
                Ok(Side {
                    id: Some(id),
                    mode: Some(mode),
                    bytes: Some(bytes),
                })
            }
            Ok(md) if md.is_symlink() => Err(fail(format!(
                "{shown} is a symbolic link: choose it as a whole"
            ))),
            Ok(_) => Err(fail(format!("{shown} is a folder, not a file"))),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(Side::none())
            }
            Err(e) => Err(fail(format!("cannot read {shown}: {e}"))),
        }
    }

    /// The two sides a diff of `side` compares.
    fn sides(
        &self,
        index: &gix::index::State,
        path: &BStr,
        shown: &str,
        side: GitPatchSide,
    ) -> Result<(Side, Side)> {
        Self::refuse_conflict(index, path, shown)?;
        match side {
            GitPatchSide::Unstaged => {
                Ok((self.index_side(index, path)?, self.work_side(path, shown)?))
            }
            GitPatchSide::Staged => Ok((self.head_side(path)?, self.index_side(index, path)?)),
        }
    }

    /// Diff of one file between `HEAD` and the index (`staged`) or the index and the working
    /// tree (`unstaged`), with the blob ids to name back in a [`GitPatchRequest`].
    pub fn index_diff(&self, path: &str, side: GitPatchSide) -> Result<GitIndexDiff> {
        let rel = self.index_path_of(path)?;
        let shown = self.api_path(rel.as_bstr());
        let index = self.load_index()?;
        let (old, new) = self.sides(&index, rel.as_bstr(), &shown, side)?;
        let mut diff = text_diff(&shown, old.bytes.as_deref(), new.bytes.as_deref());
        if !diff.binary
            && (old.bytes.as_deref().is_some_and(|b| b.contains(&0))
                || new.bytes.as_deref().is_some_and(|b| b.contains(&0)))
        {
            diff.binary = true;
            diff.hunks.clear();
        }
        Ok(GitIndexDiff {
            side,
            diff,
            old: old.id_text(),
            new: new.id_text(),
        })
    }

    /// The stat to store for `id` at `path`: the file's own if it holds exactly that blob,
    /// else none (so Git looks at the file again).
    fn claimed_stat(&self, path: &BStr, id: ObjectId) -> Stat {
        match self.work_side(path, "") {
            Ok(Side { id: Some(w), .. }) if w == id => {
                stat_of(&fs_path(&self.workdir, path).unwrap_or_default())
            }
            _ => Stat::default(),
        }
    }

    fn set_entry(
        &self,
        index: &mut gix::index::File,
        path: &BStr,
        entry: Option<(ObjectId, Mode)>,
    ) {
        index.remove_entries(|_, p, _| p == path);
        if let Some((id, mode)) = entry {
            let stat = self.claimed_stat(path, id);
            index.dangerously_push_entry(stat, id, Flags::empty(), mode, path);
        }
    }

    /// Adds whole files to the index as they are in the working tree (a deleted file: its
    /// removal); a conflicted file counts as resolved, like `git add`.
    pub fn stage(&self, paths: &[String]) -> Result<()> {
        let mut index = self.load_index()?;
        let (mut pipeline, _) = self
            .repo
            .filter_pipeline(None)
            .ctx("cannot read the attributes")?;
        for path in paths {
            let rel = self.index_path_of(path)?;
            let shown = self.api_path(rel.as_bstr());
            match pipeline
                .worktree_file_to_object(rel.as_bstr(), &index)
                .ctx("cannot read a file")?
            {
                Some((_, gix::objs::tree::EntryKind::Commit, _)) => {
                    return Err(fail(format!("{shown} is a nested repository")));
                }
                Some((id, kind, _)) => {
                    let mode = crate::commit::index_mode(kind);
                    self.set_entry(&mut index, rel.as_bstr(), Some((id, mode)));
                }
                None => {
                    let file = fs_path(&self.workdir, rel.as_bstr())?;
                    if file.is_dir() {
                        return Err(fail(format!("{shown} is a folder, not a file")));
                    }
                    if !index
                        .entries()
                        .iter()
                        .any(|e| e.path(&index) == rel.as_bstr())
                    {
                        return Err(fail(format!(
                            "{shown} is neither in the working tree nor staged"
                        )));
                    }
                    self.set_entry(&mut index, rel.as_bstr(), None);
                }
            }
        }
        self.write_index(&mut index)
    }

    /// Puts whole files in the index back to what `HEAD` has (a new file leaves the index).
    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        let mut index = self.load_index()?;
        for path in paths {
            let rel = self.index_path_of(path)?;
            let head = self.head_side(rel.as_bstr())?;
            let entry = head.id.zip(head.mode);
            self.set_entry(&mut index, rel.as_bstr(), entry);
        }
        self.write_index(&mut index)
    }

    /// Reads both sides of `patch`, refuses a stale selection and maps it onto the lines.
    fn prepare_patch(
        &self,
        index: &gix::index::State,
        patch: &GitPatchRequest,
        side: GitPatchSide,
    ) -> Result<(BString, String, Side, Side, String, String, Selection)> {
        if patch.side != side {
            return Err(fail("this change works on the other side of the file"));
        }
        let rel = self.index_path_of(&patch.path)?;
        let shown = self.api_path(rel.as_bstr());
        let (old, new) = self.sides(index, rel.as_bstr(), &shown, side)?;
        if old.id_text() != patch.old || new.id_text() != patch.new {
            return Err(fail(format!(
                "{shown} changed since you chose these lines: reload it and choose again"
            )));
        }
        let old_text = text_of(&old, &shown)?;
        let new_text = text_of(&new, &shown)?;
        let selection = Selection::of(&old_text, &new_text, &patch.hunks)?;
        Ok((rel, shown, old, new, old_text, new_text, selection))
    }

    /// Stages the chosen lines of the working file.
    pub fn stage_patch(&self, patch: &GitPatchRequest) -> Result<()> {
        let mut index = self.load_index()?;
        let (rel, _, old, new, old_text, new_text, selection) =
            self.prepare_patch(&index, patch, GitPatchSide::Unstaged)?;
        if selection.is_empty() {
            return Ok(());
        }
        let result = selection.render(&old_text, &new_text, false);
        let entry = if new.bytes.is_none() && result.is_empty() {
            // Every line of a deleted file: the deletion.
            None
        } else {
            let mode = old.mode.or(new.mode).unwrap_or(Mode::FILE);
            let id = self
                .repo
                .write_blob(result.as_bytes())
                .ctx("cannot write a file")?
                .detach();
            Some((id, mode))
        };
        self.set_entry(&mut index, rel.as_bstr(), entry);
        self.write_index(&mut index)
    }

    /// Takes the chosen lines out of the index (back to `HEAD`).
    pub fn unstage_patch(&self, patch: &GitPatchRequest) -> Result<()> {
        let mut index = self.load_index()?;
        let (rel, _, old, new, old_text, new_text, selection) =
            self.prepare_patch(&index, patch, GitPatchSide::Staged)?;
        if selection.is_empty() {
            return Ok(());
        }
        let result = selection.render(&old_text, &new_text, true);
        let entry = match old.bytes.as_deref() {
            // Everything unstaged: exactly what HEAD has.
            Some(head) if head == result.as_bytes() => old.id.zip(old.mode),
            None if result.is_empty() => None,
            _ => {
                let mode = new.mode.or(old.mode).unwrap_or(Mode::FILE);
                let id = self
                    .repo
                    .write_blob(result.as_bytes())
                    .ctx("cannot write a file")?
                    .detach();
                Some((id, mode))
            }
        };
        self.set_entry(&mut index, rel.as_bstr(), entry);
        self.write_index(&mut index)
    }

    /// Takes the chosen lines out of the working file (back to the index). The file as it
    /// was is kept first under a backup ref (`refs/requesttailor/backup/<unix>/discard…`,
    /// a commit of `HEAD` with that one file as it was).
    pub fn discard_patch(&self, patch: &GitPatchRequest) -> Result<()> {
        let index = self.load_index()?;
        let (rel, shown, old, new, old_text, new_text, selection) =
            self.prepare_patch(&index, patch, GitPatchSide::Unstaged)?;
        if selection.is_empty() {
            return Ok(());
        }
        if old.bytes.is_none() {
            return Err(fail(format!(
                "{shown} is not tracked yet: there is nothing to go back to"
            )));
        }
        let result = selection.render(&old_text, &new_text, true);
        self.back_up_file(rel.as_bstr(), new.bytes.as_deref())?;
        let file = fs_path(&self.workdir, rel.as_bstr())?;
        std::fs::write(&file, result.as_bytes()).ctx("cannot write a file")?;
        Ok(())
    }

    fn back_up_file(&self, path: &BStr, content: Option<&[u8]>) -> Result<()> {
        let head = self.head_tree()?;
        let mut editor = self.repo.edit_tree(head).ctx("cannot build the tree")?;
        match content {
            Some(bytes) => {
                let id = self
                    .repo
                    .write_blob(bytes)
                    .ctx("cannot write a file")?
                    .detach();
                editor
                    .upsert(path, gix::objs::tree::EntryKind::Blob, id)
                    .ctx("cannot build the tree")?;
            }
            None => {
                editor.remove(path).ctx("cannot build the tree")?;
            }
        }
        let tree = editor.write().ctx("cannot write the tree")?.detach();
        let parents = self.head_commit()?.into_iter().collect();
        let commit = crate::undo::write_commit(
            self,
            tree,
            parents,
            &format!("Backup before discarding lines of {path}"),
            None,
        )?;
        let unix = crate::actions::now();
        for n in 0.. {
            let name = if n == 0 {
                rt_app::git::actions::backup_ref(unix, "discard")
            } else {
                rt_app::git::actions::backup_ref(unix, &format!("discard-{n}"))
            };
            if self.ref_commit(&name)?.is_none() {
                return self.put_ref(&name, Some(commit), None, "backup before discarding lines");
            }
        }
        Ok(())
    }

    /// The staged files that differ from `HEAD`, as text where they are text (for the secret
    /// scan), by repository path.
    fn staged_texts(&self, index: &gix::index::State) -> Result<Vec<(String, String)>> {
        let head = crate::util::flatten(&self.repo, self.head_tree()?)?;
        let mut out = Vec::new();
        for entry in index.entries() {
            if entry.stage() != Stage::Unconflicted || entry.mode == Mode::COMMIT {
                continue;
            }
            let path = entry.path(index);
            if head.get(path).is_some_and(|b| b.id == entry.id) {
                continue;
            }
            let bytes = blob_bytes(&self.repo, entry.id)?;
            if let Some(text) = secret_scan::text_of(&bytes) {
                out.push((self.api_path(path), text.to_owned()));
            }
        }
        Ok(out)
    }

    /// Commits exactly what the index holds on top of `HEAD` (while a merge, cherry-pick or
    /// revert is underway: concluding it). The staged files go through the secret scanner
    /// first (`AppError::SecretsFound`). The identity comes from the Git configuration, else
    /// `fallback`.
    pub fn commit_staged(
        &self,
        commit: &GitStagedCommit,
        fallback: Option<&rt_app::git::GitIdentity>,
    ) -> Result<GitCommit> {
        let message = commit.message.trim();
        if message.is_empty() {
            return Err(fail("the commit message is empty"));
        }
        let underway = self.underway()?.is_some();
        let merge_parent = self.merge_head()?;
        let index = self.load_index()?;
        if index
            .entries()
            .iter()
            .any(|e| e.stage() != Stage::Unconflicted)
        {
            return Err(fail("resolve all conflicts before committing"));
        }
        let tree = self.tree_from_index(&index)?;
        let head = self.head_commit()?;
        if !underway && tree == self.head_tree()? {
            return Err(fail("nothing is staged"));
        }
        let texts = self.staged_texts(&index)?;
        let files: Vec<(&str, &str)> = texts
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        let findings = secret_scan::scan(&files);
        if !findings.is_empty() {
            return Err(AppError::SecretsFound(secret_scan::refusal(&findings)));
        }
        let identity = self.identity_or(fallback)?;
        let sig = signature(&identity);
        let parents: Vec<ObjectId> = head.into_iter().chain(merge_parent).collect();
        let id = self.commit_on("HEAD", &sig, &sig, message, tree, parents)?;
        if underway {
            self.clear_merge_state()?;
        }
        describe(&self.repo, id)
    }
}

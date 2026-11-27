// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Undo of a Git action (wave 14, strand G) from its journal entry, and the snapshots of the
//! working files that make it possible.
//!
//! Undo is refused once a ref the action moved points elsewhere (`stale`: newer work would be
//! lost), once `HEAD` is on another branch, or when the configuration or the stash changed
//! since. Otherwise it puts the refs back, `HEAD`, the configuration and the stash list, and
//! moves the working files from the state the action left to the state before it. Files
//! edited since then block the undo like a checkout would (nothing is overwritten).

use std::collections::{BTreeMap, BTreeSet};

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::{Flags, Mode, Stage, Stat};
use serde::{Deserialize, Serialize};
use tk_git_types::actions::GitActionResult;
use tk_git_types::{GitFile, GitFileState, GitIdentity};

use crate::actions::{JournalLine, Restore};
use crate::repo::Repo;
use crate::util::{Context, Result, fail, flatten};
use crate::worktree::{blocked, blocked_error, switch};

/// `base` with `files` (repository-relative, as `status` lists them) as they are on disk:
/// written, or removed when deleted. Blobs are written.
pub(crate) fn tree_with(whole: &Repo, base: ObjectId, files: &[&GitFile]) -> Result<ObjectId> {
    if files.is_empty() {
        return Ok(base);
    }
    let index = whole.load_index()?;
    let mut editor = whole.repo.edit_tree(base).ctx("cannot read a tree")?;
    let mut pipeline = crate::filter::Filters::new(whole)?;
    for f in files {
        let path = BString::from(f.path.as_str());
        if f.state == GitFileState::Deleted {
            editor.remove(path.as_bstr()).ctx("cannot build the tree")?;
            continue;
        }
        // Outside the sparse checkout: what the index holds.
        if let Some(b) = crate::sparse::assumed(path.as_bstr(), &index) {
            editor
                .upsert(path.as_bstr(), crate::commit::entry_kind(b.mode), b.id)
                .ctx("cannot build the tree")?;
            continue;
        }
        match pipeline.worktree_file_to_object(path.as_bstr(), &index)? {
            Some((_, gix::objs::tree::EntryKind::Commit, _)) => {}
            Some((id, kind, _)) => {
                editor
                    .upsert(path.as_bstr(), kind, id)
                    .ctx("cannot build the tree")?;
            }
            None => {
                editor.remove(path.as_bstr()).ctx("cannot build the tree")?;
            }
        }
    }
    Ok(editor.write().ctx("cannot write the tree")?.detach())
}

/// The working files as a tree (blobs are written): `HEAD`'s tree with every changed,
/// deleted, conflicted and untracked (not ignored) file as it is on disk. `whole` is the
/// repository without the scope.
pub(crate) fn snapshot_tree(whole: &Repo) -> Result<ObjectId> {
    let files = whole.status()?.files;
    let all: Vec<&GitFile> = files.iter().collect();
    tree_with(whole, whole.head_tree()?, &all)
}

/// Writes a commit that no ref points to (yet), by `identity` (else the product's own).
/// Never signed: it is for backups and snapshots (see [`write_signed_commit`]).
pub(crate) fn write_commit(
    repo: &Repo,
    tree: ObjectId,
    parents: Vec<ObjectId>,
    message: &str,
    identity: Option<&GitIdentity>,
) -> Result<ObjectId> {
    write_commit_with(repo, tree, parents, message, identity, false)
}

/// As [`write_commit`], but through the repository's signer (if one is set), like
/// `Repo::commit`: for commits a user keeps, such as stash commits.
pub(crate) fn write_signed_commit(
    repo: &Repo,
    tree: ObjectId,
    parents: Vec<ObjectId>,
    message: &str,
    identity: Option<&GitIdentity>,
) -> Result<ObjectId> {
    write_commit_with(repo, tree, parents, message, identity, true)
}

fn write_commit_with(
    repo: &Repo,
    tree: ObjectId,
    parents: Vec<ObjectId>,
    message: &str,
    identity: Option<&GitIdentity>,
    sign: bool,
) -> Result<ObjectId> {
    let sig = match identity {
        Some(id) => crate::commit::signature(id),
        None => gix::actor::Signature {
            name: repo.product().name.into(),
            email: repo.product().email.into(),
            time: gix::date::Time::now_local_or_utc(),
        },
    };
    let commit = gix::objs::Commit {
        tree,
        parents: parents.into(),
        author: sig.clone(),
        committer: sig,
        encoding: None,
        message: message.into(),
        extra_headers: Vec::new(),
    };
    if sign {
        return repo.write_commit_object(commit);
    }
    Ok(repo
        .repo
        .write_object(&commit)
        .ctx("cannot write a commit")?
        .detach())
}

/// A commit of the working files ([`snapshot_tree`]) with `HEAD` as parent.
pub(crate) fn snapshot_commit(whole: &Repo, message: &str) -> Result<ObjectId> {
    let tree = snapshot_tree(whole)?;
    let parents = whole.head_commit()?.into_iter().collect();
    write_commit(whole, tree, parents, message, None)
}

/// How the index differed from `HEAD`'s tree before an action (private to the journal): undo
/// writes the index for `HEAD` and puts these entries back, so staged changes and conflict
/// stages survive an action and its undo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct IndexDelta {
    /// Entries `HEAD`'s tree does not have as they are: `(path, mode, id, stage)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<(String, u32, String, u8)>,
    /// Paths of `HEAD`'s tree without an unconflicted entry in the index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// How a recorded change left the index (private to the journal).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct IndexMove {
    /// Its difference from `HEAD`'s tree after the change; `None`: just that tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<IndexDelta>,
}

impl Repo {
    /// How the index differs from `HEAD`'s tree; `None` when it is just that tree.
    pub(crate) fn index_delta(&self) -> Result<Option<IndexDelta>> {
        let index = self.load_index()?;
        let head = self
            .repo
            .index_from_tree(&self.head_tree()?)
            .ctx("cannot build the index")?;
        let mut want: BTreeMap<&[u8], (Mode, ObjectId)> = BTreeMap::new();
        for e in head.entries() {
            want.insert(e.path(&head).as_bytes(), (e.mode, e.id));
        }
        let mut delta = IndexDelta::default();
        let mut seen: BTreeSet<&[u8]> = BTreeSet::new();
        for e in index.entries() {
            let path = e.path(&index).as_bytes();
            let stage = e.stage();
            if stage == Stage::Unconflicted {
                seen.insert(path);
                if want.get(path) == Some(&(e.mode, e.id)) {
                    continue;
                }
            }
            delta.entries.push((
                path.to_str_lossy().into_owned(),
                e.mode.bits(),
                e.id.to_string(),
                stage as u8,
            ));
        }
        for path in want.keys() {
            if !seen.contains(path) {
                delta.missing.push(path.to_str_lossy().into_owned());
            }
        }
        Ok((!delta.entries.is_empty() || !delta.missing.is_empty()).then_some(delta))
    }

    /// Puts the entries of `delta` back into the index (which is `HEAD`'s tree now).
    pub(crate) fn apply_index_delta(&self, delta: &IndexDelta) -> Result<()> {
        let mut index = self.load_index()?;
        let paths: BTreeSet<BString> = delta
            .entries
            .iter()
            .map(|(p, ..)| BString::from(p.as_str()))
            .chain(delta.missing.iter().map(|p| BString::from(p.as_str())))
            .collect();
        index.remove_entries(|_, p, _| paths.contains(p));
        for (path, mode, id, stage) in &delta.entries {
            let id = ObjectId::from_hex(id.as_bytes()).ctx("invalid journal")?;
            let mode = Mode::from_bits(*mode).ok_or_else(|| fail("invalid journal"))?;
            let stage = match stage {
                0 => Stage::Unconflicted,
                1 => Stage::Base,
                2 => Stage::Ours,
                3 => Stage::Theirs,
                _ => return Err(fail("invalid journal")),
            };
            index.dangerously_push_entry(
                Stat::default(),
                id,
                Flags::from_stage(stage),
                mode,
                BString::from(path.as_str()).as_bstr(),
            );
        }
        self.write_index(&mut index)
    }

    /// The index for `tree` (normally `HEAD`'s), keeping every stat that is still a true
    /// claim (same path, same blob) from `claims`, so unchanged files are not hashed again and
    /// changed ones always are.
    pub(crate) fn reindex(
        &self,
        tree: ObjectId,
        claims: &[&gix::index::State],
    ) -> Result<gix::index::File> {
        let mut index = self
            .repo
            .index_from_tree(&tree)
            .ctx("cannot build the index")?;
        let path = self.repo.index_path();
        index.set_path(path);
        let (entries, backing) = index.entries_mut_and_pathbacking();
        for entry in entries {
            let p = entry.path_in(backing);
            for claim in claims {
                if let Some(prev) = claim.entry_by_path_and_stage(p, Stage::Unconflicted)
                    && prev.id == entry.id
                    && prev.mode == entry.mode
                {
                    entry.stat = prev.stat;
                    break;
                }
            }
        }
        // Skip-worktree flags (sparse checkout) survive the rebuild.
        let mut skipped = std::collections::BTreeSet::new();
        for claim in claims {
            skipped.extend(crate::sparse::skipped(claim));
        }
        crate::sparse::keep(&mut index, &skipped);
        Ok(index)
    }

    /// Moves the working files from tree `from` (what they are now) to tree `to`, then
    /// writes the index for `HEAD`. Refuses (and changes nothing) when a file to change was
    /// edited since `from`.
    pub(crate) fn move_worktree(&self, from: ObjectId, to: ObjectId, what: &str) -> Result<()> {
        let old = self.load_index()?;
        let moved = switch(self, from, to, what)?;
        let head = self.head_tree()?;
        let mut index = self.reindex(head, &[&moved, &old])?;
        self.write_index(&mut index)
    }

    /// Why the journal entry cannot be undone any more, in plain words.
    pub(crate) fn staleness(&self, line: &JournalLine) -> Result<Option<String>> {
        let mut names: Vec<String> = Vec::new();
        for r in &line.entry.refs {
            let now = self.ref_value(&r.name)?.map(|id| id.to_string());
            if now != r.after {
                let name = if r.name == "HEAD" {
                    "the current commit".to_owned()
                } else {
                    r.name
                        .strip_prefix("refs/heads/")
                        .or_else(|| r.name.strip_prefix("refs/tags/"))
                        .unwrap_or(&r.name)
                        .to_owned()
                };
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        if let Some((_, after)) = &line.restore.head {
            let now = self.head_ref()?.map(|n| n.as_bstr().to_string());
            if &now != after && !names.iter().any(|n| n == "the current commit") {
                names.push("the current branch".into());
            }
        }
        if !names.is_empty() {
            return Ok(Some(format!(
                "{} changed since, so undoing would lose newer work.",
                names.join(", ")
            )));
        }
        if let Some(pushed) = &line.restore.pushed {
            let tracking = line.restore.remote_refs.iter().find(|m| {
                m.after == pushed.after
                    && m.name
                        .starts_with(&format!("refs/remotes/{}/", pushed.remote))
            });
            if let Some(t) = tracking
                && self.ref_value(&t.name)?.map(|id| id.to_string()) != pushed.after
            {
                return Ok(Some(
                    "The branch on the server changed since, so undoing the push would lose newer work. Make a commit that reverts it instead."
                        .into(),
                ));
            }
        }
        if let Some(moved) = &line.restore.index_moved
            && self.index_delta()? != moved.after
        {
            return Ok(Some(
                "The staged changes changed since, so undoing would lose newer work.".into(),
            ));
        }
        if let Some((_, after)) = &line.restore.config
            && &self.config_text()? != after
        {
            return Ok(Some(
                "The repository settings changed since, so undoing would lose newer changes."
                    .into(),
            ));
        }
        if let Some((_, after)) = &line.restore.stash_log
            && &self.stash_log_text()? != after
        {
            return Ok(Some(
                "The parked changes changed since, so undoing would lose newer work.".into(),
            ));
        }
        Ok(None)
    }

    /// What the journal entry `id` moved on a remote, as (remote, ref on the remote), when it is
    /// an undoable push or publish ([`Repo::with_push_undo`]): a product checks its own rules
    /// (protected branches) before it undoes one.
    pub fn pushed_ref(&self, id: &str) -> Result<Option<(String, String)>> {
        Ok(self
            .journal()?
            .into_iter()
            .find(|l| l.entry.id == id)
            .and_then(|l| l.restore.pushed)
            .map(|p| (p.remote, p.remote_ref)))
    }

    /// Takes back the journal entry `id` (see the module docs). `unsaved`: the app has
    /// unsaved edits (refused). The undo of a push needs credentials: [`Repo::undo_with`].
    pub fn undo(&self, id: &str, unsaved: bool) -> Result<GitActionResult> {
        self.undo_with(id, unsaved, &crate::credentials::NoCredentials)
    }

    /// [`Repo::undo`] with credentials for the remote: the undo of a push or publish
    /// ([`Repo::with_push_undo`]) sets the branch on the remote back to what it was, with a lease
    /// (only while it still points at what was pushed), before anything local changes.
    pub fn undo_with(
        &self,
        id: &str,
        unsaved: bool,
        creds: &dyn crate::credentials::CredentialSource,
    ) -> Result<GitActionResult> {
        let mut journal = self.journal()?;
        let Some(at) = journal.iter().position(|l| l.entry.id == id) else {
            return Err(fail("This action can no longer be undone."));
        };
        let line = journal[at].clone();
        if let Some(stale) = self.staleness(&line)? {
            return Err(fail(format!("Cannot undo: {stale}")));
        }
        if unsaved {
            return Err(fail("Save your changes in the app first."));
        }
        let whole = self.whole()?;
        let r: &Restore = &line.restore;
        // The working files first, as a dry run: nothing moves when they were edited since.
        let worktree = match &r.worktree {
            Some((before, after)) => {
                let before = ObjectId::from_hex(before.as_bytes()).ctx("invalid journal")?;
                let after = ObjectId::from_hex(after.as_bytes()).ctx("invalid journal")?;
                let before_tree = whole.commit_tree(before)?;
                let (from, to) = (
                    flatten(&whole.repo, after)?,
                    flatten(&whole.repo, before_tree)?,
                );
                let index = whole.load_index()?;
                let stuck = blocked(&whole, &from, &to, &index)?;
                if !stuck.is_empty() {
                    return Err(blocked_error(&whole, "undo", &stuck));
                }
                Some((after, before_tree))
            }
            None => None,
        };
        let head_before = self.head_commit()?;

        // The remote first: when it refuses (someone pushed on top), nothing local changes.
        if let Some(pushed) = &r.pushed {
            let after = pushed
                .after
                .as_deref()
                .map(|h| ObjectId::from_hex(h.as_bytes()).ctx("invalid journal"))
                .transpose()?;
            let before = pushed
                .before
                .as_deref()
                .map(|h| ObjectId::from_hex(h.as_bytes()).ctx("invalid journal"))
                .transpose()?;
            self.update_remote_ref(
                &pushed.remote,
                before,
                &pushed.remote_ref,
                Some(after),
                creds,
            )
            .map_err(|e| match e {
                crate::GitError::Auth(_) => e,
                other => fail(format!(
                    "Cannot undo the push: {other}. Make a commit that reverts it instead."
                )),
            })?;
        }

        // Refs: first those that get a value back, then HEAD, then those to delete.
        let parse = |v: &Option<String>| -> Result<Option<ObjectId>> {
            v.as_deref()
                .map(|h| ObjectId::from_hex(h.as_bytes()).ctx("invalid journal"))
                .transpose()
        };
        let message = self.reflog_message(&format!("undo {}", line.entry.action));
        let stash_handled = r.stash_log.is_some();
        for m in &line.entry.refs {
            if m.name == "HEAD" || (stash_handled && m.name == "refs/stash") {
                continue;
            }
            if m.before.is_some() {
                self.put_ref(&m.name, parse(&m.before)?, parse(&m.after)?, &message)?;
            }
        }
        match &r.head {
            Some((Some(branch), _)) => {
                let full = gix::refs::FullName::try_from(branch.as_str())
                    .map_err(|_| fail("invalid journal"))?;
                self.set_head(&full, &message)?;
            }
            Some((None, _)) => {
                let target = line
                    .entry
                    .refs
                    .iter()
                    .find(|m| m.name == "HEAD")
                    .and_then(|m| m.before.clone())
                    .or_else(|| head_before.map(|h| h.to_string()));
                if let Some(t) = target {
                    let id = ObjectId::from_hex(t.as_bytes()).ctx("invalid journal")?;
                    self.detach_head(id, &message)?;
                }
            }
            None => {
                // Detached all along: HEAD itself moved.
                if self.head_ref()?.is_none()
                    && let Some(m) = line.entry.refs.iter().find(|m| m.name == "HEAD")
                    && let Some(b) = parse(&m.before)?
                {
                    self.detach_head(b, &message)?;
                }
            }
        }
        for m in &line.entry.refs {
            if m.name == "HEAD" || (stash_handled && m.name == "refs/stash") {
                continue;
            }
            if m.before.is_none() {
                self.put_ref(&m.name, None, parse(&m.after)?, &message)?;
            }
        }
        for m in &r.remote_refs {
            let now = self.ref_value(&m.name)?;
            self.put_ref(&m.name, parse(&m.before)?, now, &message)?;
        }
        if let Some((before, _)) = &r.stash_log {
            self.restore_stash_log(before)?;
        }
        if let Some((before, _)) = &r.config {
            self.write_config_text(before)?;
        }
        if r.underway || !r.merge.is_empty() {
            self.clear_merge_state()?;
        }
        // A merge the action stopped is in progress again.
        self.write_merge_files(&r.merge)?;
        // So is a rebase (or the rebase the action started is gone).
        if let Some((before, after)) = &r.rebase {
            self.restore_rebase_files(before, after)?;
        }
        let repo = self.reopen()?;
        let head_after = repo.head_commit()?;
        let mut reload = head_before != head_after;
        let mut reindexed = true;
        if let Some((from, to)) = worktree {
            let whole = repo.whole()?;
            whole.move_worktree(from, to, "undo")?;
            reload = true;
        } else if reload
            || r.underway
            || !r.merge.is_empty()
            || r.rebase.is_some()
            || r.index_moved.is_some()
        {
            let old = repo.load_index()?;
            let mut index = repo.reindex(repo.head_tree()?, &[&old])?;
            repo.write_index(&mut index)?;
        } else {
            reindexed = false;
        }
        // The index is `HEAD`'s tree now: what was staged (or conflicted) before comes back.
        if reindexed && let Some(delta) = &r.index {
            repo.apply_index_delta(delta)?;
        }
        journal.remove(at);
        self.write_journal(&journal)?;
        Ok(GitActionResult {
            message: format!("Undone: {}", line.entry.summary),
            undo: None,
            outcome: None,
            reload,
        })
    }
}

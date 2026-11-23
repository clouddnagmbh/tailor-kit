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

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::Stage;
use rt_app::git::actions::GitActionResult;
use rt_app::git::{GitFile, GitFileState, GitIdentity};

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
    let (mut pipeline, _) = whole
        .repo
        .filter_pipeline(None)
        .ctx("cannot read the attributes")?;
    for f in files {
        let path = BString::from(f.path.as_str());
        if f.state == GitFileState::Deleted {
            editor.remove(path.as_bstr()).ctx("cannot build the tree")?;
            continue;
        }
        match pipeline
            .worktree_file_to_object(path.as_bstr(), &index)
            .ctx("cannot read a file")?
        {
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
/// repository without collection scope.
pub(crate) fn snapshot_tree(whole: &Repo) -> Result<ObjectId> {
    let files = whole.status()?.files;
    let all: Vec<&GitFile> = files.iter().collect();
    tree_with(whole, whole.head_tree()?, &all)
}

/// Writes a commit that no ref points to (yet), by `identity` (else RequestTailor).
pub(crate) fn write_commit(
    repo: &Repo,
    tree: ObjectId,
    parents: Vec<ObjectId>,
    message: &str,
    identity: Option<&GitIdentity>,
) -> Result<ObjectId> {
    let sig = match identity {
        Some(id) => crate::commit::signature(id),
        None => gix::actor::Signature {
            name: "RequestTailor".into(),
            email: "requesttailor@localhost".into(),
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

impl Repo {
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

    /// Takes back the journal entry `id` (see the module docs). `unsaved`: the app has
    /// unsaved edits (refused).
    pub fn undo(&self, id: &str, unsaved: bool) -> Result<GitActionResult> {
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

        // Refs: first those that get a value back, then HEAD, then those to delete.
        let parse = |v: &Option<String>| -> Result<Option<ObjectId>> {
            v.as_deref()
                .map(|h| ObjectId::from_hex(h.as_bytes()).ctx("invalid journal"))
                .transpose()
        };
        let message = format!("requesttailor: undo {}", line.entry.action);
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
        if r.underway {
            self.clear_merge_state()?;
        }
        let repo = self.reopen()?;
        let head_after = repo.head_commit()?;
        let mut reload = head_before != head_after;
        if let Some((from, to)) = worktree {
            let whole = repo.whole()?;
            whole.move_worktree(from, to, "undo")?;
            reload = true;
        } else if reload || r.underway {
            let old = repo.load_index()?;
            let mut index = repo.reindex(repo.head_tree()?, &[&old])?;
            repo.write_index(&mut index)?;
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

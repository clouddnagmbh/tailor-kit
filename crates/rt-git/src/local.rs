// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Remotes on this machine (a path or `file://`): fetch and push in-process. The other
//! repository is opened with gitoxide and the objects it lacks are copied over; no `git`
//! program is spawned (gitoxide's own `file://` transport would run `git-upload-pack`).
//!
//! The object walk ([`missing_objects`]) is shared with the HTTP push, which sends the same
//! objects as a pack.

use std::collections::{HashSet, VecDeque};
use std::path::Path;

use gix::ObjectId;
use gix::objs::Write as _;
use gix::refs::transaction::PreviousValue;

use crate::util::{Context, Result, fail};

/// Objects reachable from `tips` that the other side lacks, by `has` (which answers for the
/// other side). Assumes the other side is complete: a commit it has comes with its history,
/// a tree with everything below it. Order: newest commit first, each followed by its trees
/// and blobs; write in reverse so nothing ever points to a missing object.
pub fn missing_objects(
    src: &gix::Repository,
    tips: &[ObjectId],
    has: &dyn Fn(&gix::oid) -> bool,
) -> Result<Vec<ObjectId>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut commits: VecDeque<ObjectId> = tips.iter().copied().collect();
    while let Some(id) = commits.pop_front() {
        if !seen.insert(id) || has(&id) {
            continue;
        }
        let commit = src.find_commit(id).ctx("cannot read a commit")?;
        out.push(id);
        let tree = commit.tree_id().ctx("cannot read a commit")?.detach();
        tree_objects(src, tree, has, &mut seen, &mut out)?;
        for parent in commit.parent_ids() {
            commits.push_back(parent.detach());
        }
    }
    Ok(out)
}

/// `tree` and everything below it that `has` does not know, skipping `seen`.
pub fn tree_objects(
    src: &gix::Repository,
    tree: ObjectId,
    has: &dyn Fn(&gix::oid) -> bool,
    seen: &mut HashSet<ObjectId>,
    out: &mut Vec<ObjectId>,
) -> Result<()> {
    let mut trees = vec![tree];
    while let Some(t) = trees.pop() {
        if !seen.insert(t) || has(&t) {
            continue;
        }
        out.push(t);
        let tree = src.find_tree(t).ctx("cannot read a tree")?;
        for entry in tree.decode().ctx("cannot read a tree")?.entries.iter() {
            let oid = entry.oid.to_owned();
            if entry.mode.is_tree() {
                trees.push(oid);
            } else if entry.mode.is_commit() {
                // Submodule commits live in another repository.
            } else if !has(&oid) && seen.insert(oid) {
                out.push(oid);
            }
        }
    }
    Ok(())
}

/// Copies `ids` (as returned by [`missing_objects`]) from `src` to `dst`.
pub fn copy_objects(src: &gix::Repository, dst: &gix::Repository, ids: &[ObjectId]) -> Result<()> {
    for id in ids.iter().rev() {
        let object = src.find_object(*id).ctx("cannot read an object")?;
        dst.write_buf(object.kind, &object.data)
            .ctx("cannot write an object")?;
    }
    Ok(())
}

/// Whether `ancestor` is reachable from `tip` (or equal to it).
pub fn is_ancestor(repo: &gix::Repository, ancestor: ObjectId, tip: ObjectId) -> bool {
    ancestor == tip
        || repo
            .merge_base(ancestor, tip)
            .is_ok_and(|base| base.is_some_and(|b| b.detach() == ancestor))
}

pub(crate) fn open(path: &Path, options: &crate::Options) -> Result<gix::Repository> {
    if !path.exists() {
        return Err(fail(format!(
            "the remote repository {} does not exist",
            path.display()
        )));
    }
    gix::open_opts(path, options.open()).ctx(&format!(
        "cannot open the remote repository {}",
        path.display()
    ))
}

fn peeled(repo: &gix::Repository, name: &str) -> Result<Option<ObjectId>> {
    match repo.try_find_reference(name).ctx("cannot read a branch")? {
        Some(mut r) => Ok(Some(r.peel_to_id().ctx("cannot read a branch")?.detach())),
        None => Ok(None),
    }
}

/// Sets `remote_ref` (e.g. `refs/heads/main`) in the repository at `path` to `new`, copying
/// what it lacks. Fast-forward only.
pub fn push(
    src: &gix::Repository,
    path: &Path,
    options: &crate::Options,
    new: ObjectId,
    remote_ref: &str,
) -> Result<()> {
    let dst = open(path, options)?;
    if !dst.is_bare()
        && dst
            .head_name()
            .ok()
            .flatten()
            .is_some_and(|h| h.as_bstr() == remote_ref)
    {
        return Err(fail(
            "the remote repository has this branch checked out; push to a bare repository",
        ));
    }
    let current = peeled(&dst, remote_ref)?;
    if current == Some(new) {
        return Ok(());
    }
    if let Some(cur) = current
        && (!src.has_object(cur) || !is_ancestor(src, cur, new))
    {
        return Err(fail(
            "the remote branch has commits you do not have: pull first",
        ));
    }
    let ids = missing_objects(src, &[new], &|id| dst.has_object(id))?;
    copy_objects(src, &dst, &ids)?;
    let expected = match current {
        Some(cur) => PreviousValue::ExistingMustMatch(gix::refs::Target::Object(cur)),
        None => PreviousValue::MustNotExist,
    };
    dst.reference(remote_ref, new, expected, "push")
        .ctx("cannot update the remote branch")?;
    Ok(())
}

/// Fetches every branch of the repository at `path` into `refs/remotes/<remote_name>/*`.
/// Returns the branch its `HEAD` points to.
pub fn fetch(
    dst: &gix::Repository,
    remote_name: &str,
    path: &Path,
    options: &crate::Options,
) -> Result<Option<String>> {
    let src = open(path, options)?;
    let refs = src.references().ctx("cannot list the remote branches")?;
    let mut branches = Vec::new();
    for r in refs
        .local_branches()
        .ctx("cannot list the remote branches")?
    {
        let mut r = r.map_err(|e| fail(format!("cannot list the remote branches: {e}")))?;
        let name = r.name().shorten().to_string();
        let id = r.peel_to_id().ctx("cannot read a remote branch")?.detach();
        branches.push((name, id));
    }
    let tips: Vec<ObjectId> = branches.iter().map(|(_, id)| *id).collect();
    let ids = missing_objects(&src, &tips, &|id| dst.has_object(id))?;
    copy_objects(&src, dst, &ids)?;
    for (name, id) in &branches {
        dst.reference(
            format!("refs/remotes/{remote_name}/{name}"),
            *id,
            PreviousValue::Any,
            "fetch",
        )
        .ctx("cannot update a remote-tracking branch")?;
    }
    let head = src.head_name().ok().flatten().and_then(|h| {
        h.as_bstr()
            .strip_prefix(b"refs/heads/")
            .map(|b| String::from_utf8_lossy(b).into_owned())
    });
    Ok(head)
}

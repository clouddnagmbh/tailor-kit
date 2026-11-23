// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The commit graph of the whole repository (wave 14, strand F): `GitApi.graph`, `refs`,
//! `changes` and `fileDiff` (`rt_app::git::graph`).
//!
//! Generic: nothing here assumes the repository is a collection. Paths in and out are
//! repository-relative with a leading `/`; which of them belong to a collection of the set is
//! the shell's to say ([`rt_app::git::graph::assign_collections`]).
//!
//! Order: every commit reachable from a ref (or from the refs asked for), children before
//! their parents, ties by commit time, newest first (`git log --date-order`). The order only
//! depends on the commits, so pages (`skip`/`limit`) stay stable while no ref moves. Each page
//! walks the history again; that is a few milliseconds per thousand commits and keeps the
//! shell stateless.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};

use gix::ObjectId;
use gix::bstr::{BStr, BString, ByteSlice};
use gix::index::entry::Mode;
use rt_app::git::graph::{
    GitChangedFile, GitGraphCommit, GitGraphPage, GitGraphQuery, GitRef, GitRefKind,
};
use rt_app::git::{GitCommit, GitDiff, GitFileChange, text_diff};

use crate::file_log::{SIMILARITY, similarity};
use crate::repo::Repo;
use crate::util::{Blob, Context, Flat, Result, Scope, blob_bytes, fail, flatten};
use crate::worktree::fs_path;

/// Bytes looked at to tell binary from text (as Git does).
const SNIFF: usize = 8000;
/// Renames with edits are looked for only between files up to this size …
const RENAME_MAX_BYTES: usize = 512 * 1024;
/// … and only when there are at most this many added × deleted pairs.
const RENAME_MAX_PAIRS: usize = 10_000;

/// Whether bytes are binary for display: a NUL byte near the start, or not UTF-8.
fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(SNIFF)].contains(&0) || std::str::from_utf8(bytes).is_err()
}

/// `/path` → `path`, checked like every other path (no `..`, no `.git`).
fn repo_path(path: &str) -> Result<BString> {
    Scope::new(BString::default()).abs(path.strip_prefix('/').unwrap_or(path))
}

fn shown(path: &BStr) -> String {
    format!("/{}", path.to_str_lossy())
}

fn kind_order(kind: GitRefKind) -> u8 {
    match kind {
        GitRefKind::Head => 0,
        GitRefKind::Local => 1,
        GitRefKind::Remote => 2,
        GitRefKind::Tag => 3,
    }
}

impl Repo {
    /// Every branch, remote branch (without `<remote>/HEAD`) and tag, and `HEAD` while
    /// detached: `HEAD` first, then local branches, remote branches and tags, each by name.
    /// Refs that do not lead to a commit (a tag on a tree) are left out.
    pub fn refs(&self) -> Result<Vec<GitRef>> {
        let current = self.head_ref()?;
        let mut out = Vec::new();
        let platform = self.repo.references().ctx("cannot list the refs")?;
        for r in platform.all().ctx("cannot list the refs")? {
            let mut r = r.map_err(|e| fail(format!("cannot list the refs: {e}")))?;
            let full = r.name().as_bstr().to_str_lossy().into_owned();
            let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                (GitRefKind::Local, n.to_owned())
            } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                if n.ends_with("/HEAD") {
                    continue;
                }
                (GitRefKind::Remote, n.to_owned())
            } else if let Some(n) = full.strip_prefix("refs/tags/") {
                (GitRefKind::Tag, n.to_owned())
            } else {
                // Stashes, backups, review heads: not part of the graph.
                continue;
            };
            if r.target().try_id().is_none() {
                // A symbolic ref other than HEAD (rare): its target is listed itself.
                continue;
            }
            let direct = r.target().try_id().map(ToOwned::to_owned);
            let Ok(commit) = r.peel_to_commit() else {
                continue;
            };
            let target = commit.id;
            let message = match (kind, direct) {
                (GitRefKind::Tag, Some(id)) if id != target => self.tag_message(id),
                _ => None,
            };
            let mut entry = GitRef {
                full: full.clone(),
                name,
                kind,
                target: target.to_string(),
                current: current
                    .as_ref()
                    .is_some_and(|c| c.as_bstr() == full.as_bytes().as_bstr()),
                upstream: None,
                ahead: 0,
                behind: 0,
                message,
            };
            if kind == GitRefKind::Local
                && let Some((display, tracking)) = self.upstream_of(&entry.name)
            {
                entry.upstream = Some(display);
                if let Some(up) = self.ref_commit(&tracking)? {
                    entry.ahead = self.count_only(target, up)?;
                    entry.behind = self.count_only(up, target)?;
                }
            }
            out.push(entry);
        }
        if current.is_none()
            && let Some(head) = self.head_commit()?
        {
            out.push(GitRef {
                full: "HEAD".into(),
                name: "HEAD".into(),
                kind: GitRefKind::Head,
                target: head.to_string(),
                current: true,
                upstream: None,
                ahead: 0,
                behind: 0,
                message: None,
            });
        }
        out.sort_by(|a, b| {
            (kind_order(a.kind), &a.name).cmp(&(kind_order(b.kind), &b.name))
        });
        Ok(out)
    }

    /// The first line of an annotated tag's message.
    fn tag_message(&self, tag: ObjectId) -> Option<String> {
        let object = self.repo.find_object(tag).ok()?;
        if object.kind != gix::objs::Kind::Tag {
            return None;
        }
        let tag = object.into_tag();
        let decoded = tag.decode().ok()?;
        let first = decoded.message.lines().next()?.to_str_lossy().trim().to_owned();
        (!first.is_empty()).then_some(first)
    }

    /// The configured upstream of the local branch `branch`: (`origin/main`,
    /// `refs/remotes/origin/main`).
    fn upstream_of(&self, branch: &str) -> Option<(String, String)> {
        let remote = self.config(&format!("branch.{branch}.remote"))?;
        let merge = self.config(&format!("branch.{branch}.merge"))?;
        if remote == "." {
            return None;
        }
        let short = merge.strip_prefix("refs/heads/")?;
        Some((
            format!("{remote}/{short}"),
            format!("refs/remotes/{remote}/{short}"),
        ))
    }

    /// Commits reachable from `tip` but not from `hidden`.
    fn count_only(&self, tip: ObjectId, hidden: ObjectId) -> Result<u32> {
        if tip == hidden {
            return Ok(0);
        }
        let walk = self
            .repo
            .rev_walk([tip])
            .with_hidden([hidden])
            .all()
            .ctx("cannot walk the history")?;
        let mut n = 0u32;
        for info in walk {
            info.ctx("cannot walk the history")?;
            n += 1;
        }
        Ok(n)
    }

    /// One page of the graph (see the module docs for the order).
    pub fn commit_graph(&self, query: &GitGraphQuery) -> Result<GitGraphPage> {
        let refs = self.refs()?;
        let head = self.head_commit()?;
        let detached = head.is_some() && self.head_ref()?.is_none();
        let tips: Vec<ObjectId> = match &query.refs {
            Some(names) => names
                .iter()
                .map(|n| self.resolve_rev(n))
                .collect::<Result<_>>()?,
            None => {
                let mut tips: Vec<ObjectId> = refs
                    .iter()
                    .filter_map(|r| ObjectId::from_hex(r.target.as_bytes()).ok())
                    .collect();
                tips.extend(head);
                tips.sort();
                tips.dedup();
                tips
            }
        };
        let order = self.date_order(&tips)?;
        let mut labels: HashMap<&str, Vec<String>> = HashMap::new();
        for r in &refs {
            labels.entry(r.target.as_str()).or_default().push(r.full.clone());
        }
        let skip = query.skip as usize;
        let end = skip.saturating_add(query.limit as usize).min(order.len());
        let mut commits = Vec::with_capacity(end.saturating_sub(skip));
        for (id, parents) in order.get(skip..end).unwrap_or_default() {
            let (commit, email) = self.describe_with_email(*id)?;
            let refs = labels.remove(commit.oid.as_str()).unwrap_or_default();
            commits.push(GitGraphCommit {
                commit,
                parents: parents.iter().map(ToString::to_string).collect(),
                email,
                refs,
            });
        }
        Ok(GitGraphPage {
            commits,
            more: end < order.len(),
            refs,
            head: head.map(|h| h.to_string()),
            detached,
        })
    }

    /// All commits reachable from `tips`, children before parents, ties newest first, with
    /// their parents.
    fn date_order(&self, tips: &[ObjectId]) -> Result<Vec<(ObjectId, Vec<ObjectId>)>> {
        if tips.is_empty() {
            return Ok(Vec::new());
        }
        let walk = self
            .repo
            .rev_walk(tips.iter().copied())
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                Default::default(),
            ))
            .all()
            .ctx("cannot walk the history")?;
        struct Node {
            id: ObjectId,
            time: i64,
            parents: Vec<ObjectId>,
        }
        let mut nodes: Vec<Node> = Vec::new();
        let mut index: HashMap<ObjectId, usize> = HashMap::new();
        for info in walk {
            let info = info.ctx("cannot walk the history")?;
            index.insert(info.id, nodes.len());
            nodes.push(Node {
                id: info.id,
                time: info.commit_time.unwrap_or_default(),
                parents: info.parent_ids.iter().copied().collect(),
            });
        }
        // Kahn's algorithm: a commit is ready once all its children are out.
        let mut children = vec![0u32; nodes.len()];
        for n in &nodes {
            for p in &n.parents {
                if let Some(&i) = index.get(p) {
                    children[i] += 1;
                }
            }
        }
        // Ties by commit time, then by walk order (earlier is newer).
        let mut ready: BinaryHeap<(i64, Reverse<usize>)> = nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| children[*i] == 0)
            .map(|(i, n)| (n.time, Reverse(i)))
            .collect();
        let mut out = Vec::with_capacity(nodes.len());
        while let Some((_, Reverse(i))) = ready.pop() {
            for p in &nodes[i].parents {
                if let Some(&j) = index.get(p) {
                    children[j] -= 1;
                    if children[j] == 0 {
                        ready.push((nodes[j].time, Reverse(j)));
                    }
                }
            }
            out.push((nodes[i].id, std::mem::take(&mut nodes[i].parents)));
        }
        Ok(out)
    }

    fn describe_with_email(&self, id: ObjectId) -> Result<(GitCommit, String)> {
        let commit = self.repo.find_commit(id).ctx("cannot read a commit")?;
        let decoded = commit.decode().ctx("cannot read a commit")?;
        let author = decoded.author().ctx("cannot read a commit")?;
        Ok((
            GitCommit {
                oid: id.to_string(),
                summary: decoded.message().summary().to_str_lossy().into_owned(),
                author: author.name.to_str_lossy().into_owned(),
                time: author.seconds(),
            },
            author.email.to_str_lossy().into_owned(),
        ))
    }

    /// The first parent of `commit`, `None` for a root commit.
    fn first_parent(&self, commit: ObjectId) -> Result<Option<ObjectId>> {
        let commit = self.repo.find_commit(commit).ctx("cannot read a commit")?;
        Ok(commit.parent_ids().next().map(|id| id.detach()))
    }

    /// The files of `commit`'s tree; none for `None` (before a root commit).
    fn files_of(&self, commit: Option<ObjectId>) -> Result<Flat> {
        match commit {
            Some(c) => flatten(&self.repo, self.commit_tree(c)?),
            None => Ok(BTreeMap::new()),
        }
    }

    /// The `from` side of a comparison: the given rev, else `to`'s first parent.
    fn base_of(&self, from: Option<&str>, to: ObjectId) -> Result<Option<ObjectId>> {
        match from {
            Some(rev) => self.resolve_rev(rev).map(Some),
            None => self.first_parent(to),
        }
    }

    fn blob_binary(&self, blob: &Blob) -> Result<bool> {
        if blob.mode == Mode::COMMIT {
            return Ok(true);
        }
        Ok(is_binary(&blob_bytes(&self.repo, blob.id)?))
    }

    /// Files changed from `from` (`None`: `to`'s first parent, or nothing for a root commit)
    /// to `to`, sorted by path. A deleted and an added file with the same content, or with at
    /// least [`SIMILARITY`] of their lines in common, are one renamed file.
    pub fn changes(&self, from: Option<&str>, to: &str) -> Result<Vec<GitChangedFile>> {
        let to = self.resolve_rev(to)?;
        let from = self.base_of(from, to)?;
        let (old, new) = (self.files_of(from)?, self.files_of(Some(to))?);
        let mut out: BTreeMap<BString, GitChangedFile> = BTreeMap::new();
        let mut added: Vec<(&BString, &Blob)> = Vec::new();
        let mut deleted: Vec<(&BString, &Blob)> = Vec::new();
        for (path, blob) in &new {
            match old.get(path) {
                Some(before) if before == blob => {}
                Some(before) => {
                    let binary = self.blob_binary(before)? || self.blob_binary(blob)?;
                    out.insert(
                        path.clone(),
                        GitChangedFile {
                            path: shown(path.as_bstr()),
                            change: GitFileChange::Modified,
                            from: None,
                            binary,
                            collection: None,
                        },
                    );
                }
                None => added.push((path, blob)),
            }
        }
        for (path, blob) in &old {
            if !new.contains_key(path) {
                deleted.push((path, blob));
            }
        }
        let renames = self.renames(&added, &deleted)?;
        let renamed_from: std::collections::HashSet<&BString> = renames.values().copied().collect();
        for (path, blob) in added {
            let from = renames.get(path).map(|p| shown(p.as_bstr()));
            out.insert(
                path.clone(),
                GitChangedFile {
                    path: shown(path.as_bstr()),
                    change: if from.is_some() {
                        GitFileChange::Renamed
                    } else {
                        GitFileChange::Added
                    },
                    from,
                    binary: self.blob_binary(blob)?,
                    collection: None,
                },
            );
        }
        for (path, blob) in deleted {
            if renamed_from.contains(path) {
                continue;
            }
            out.insert(
                path.clone(),
                GitChangedFile {
                    path: shown(path.as_bstr()),
                    change: GitFileChange::Deleted,
                    from: None,
                    binary: self.blob_binary(blob)?,
                    collection: None,
                },
            );
        }
        Ok(out.into_values().collect())
    }

    /// Added path → deleted path it was renamed from: same content first, then the most
    /// similar text (ties to the smallest path).
    fn renames<'a>(
        &self,
        added: &[(&'a BString, &Blob)],
        deleted: &[(&'a BString, &Blob)],
    ) -> Result<HashMap<&'a BString, &'a BString>> {
        let mut out = HashMap::new();
        let mut taken: std::collections::HashSet<&BString> = std::collections::HashSet::new();
        for (path, blob) in added {
            if let Some((from, _)) = deleted
                .iter()
                .find(|(p, b)| b.id == blob.id && b.mode != Mode::COMMIT && !taken.contains(p))
            {
                taken.insert(from);
                out.insert(*path, *from);
            }
        }
        let open_added: Vec<_> = added.iter().filter(|(p, _)| !out.contains_key(p)).collect();
        let open_deleted: Vec<_> = deleted.iter().filter(|(p, _)| !taken.contains(p)).collect();
        if open_added.is_empty()
            || open_deleted.is_empty()
            || open_added.len() * open_deleted.len() > RENAME_MAX_PAIRS
        {
            return Ok(out);
        }
        let text = |blob: &Blob| -> Result<Option<String>> {
            if blob.mode == Mode::COMMIT {
                return Ok(None);
            }
            let bytes = blob_bytes(&self.repo, blob.id)?;
            if bytes.len() > RENAME_MAX_BYTES || is_binary(&bytes) {
                return Ok(None);
            }
            Ok(String::from_utf8(bytes).ok())
        };
        let mut old_texts = Vec::new();
        for (p, b) in &open_deleted {
            old_texts.push((*p, text(b)?));
        }
        for (path, blob) in open_added {
            let Some(new_text) = text(blob)? else {
                continue;
            };
            let mut best: Option<(f64, &BString)> = None;
            for (old_path, old_text) in &old_texts {
                let Some(old_text) = old_text else { continue };
                if taken.contains(old_path) {
                    continue;
                }
                let s = similarity(&new_text, old_text);
                if s >= SIMILARITY && best.is_none_or(|(b, _)| s > b) {
                    best = Some((s, old_path));
                }
            }
            if let Some((_, from)) = best {
                taken.insert(from);
                out.insert(*path, from);
            }
        }
        Ok(out)
    }

    /// Line diff of the repository file `path` (leading `/`) from `from` to `to`.
    /// `to` `None`: the work tree, against `from` or `HEAD`. `from` `None` with a `to`: `to`'s
    /// first parent. A file that `to` renamed is compared with its old path.
    pub fn file_diff(&self, from: Option<&str>, to: Option<&str>, path: &str) -> Result<GitDiff> {
        let rel = repo_path(path)?;
        let shown_path = shown(rel.as_bstr());
        let (base, to_id, new) = match to {
            Some(rev) => {
                let to = self.resolve_rev(rev)?;
                let base = self.base_of(from, to)?;
                let files = self.files_of(Some(to))?;
                let new = match files.get(&rel) {
                    Some(blob) if blob.mode != Mode::COMMIT => {
                        Some(blob_bytes(&self.repo, blob.id)?)
                    }
                    _ => None,
                };
                (base, Some(to), new)
            }
            None => {
                let base = match from {
                    Some(rev) => Some(self.resolve_rev(rev)?),
                    None => self.head_commit()?,
                };
                (base, None, self.work_file(rel.as_bstr(), path)?)
            }
        };
        let old_files = self.files_of(base)?;
        let mut old_blob = old_files.get(&rel).copied();
        if old_blob.is_none()
            && new.is_some()
            && let (Some(base), Some(to)) = (base, to_id)
        {
            let renamed = self
                .changes(Some(&base.to_string()), &to.to_string())?
                .into_iter()
                .find(|f| f.path == shown_path)
                .and_then(|f| f.from);
            if let Some(from_path) = renamed {
                old_blob = old_files.get(&repo_path(&from_path)?).copied();
            }
        }
        let old = match old_blob {
            Some(blob) if blob.mode != Mode::COMMIT => Some(blob_bytes(&self.repo, blob.id)?),
            _ => None,
        };
        let mut diff = text_diff(&shown_path, old.as_deref(), new.as_deref());
        if !diff.binary
            && (old.as_deref().is_some_and(is_binary) || new.as_deref().is_some_and(is_binary))
        {
            diff.binary = true;
            diff.hunks.clear();
        }
        Ok(diff)
    }

    /// A file of the work tree (symbolic links as their target), `None` if it is not there.
    fn work_file(&self, rel: &BStr, shown_as: &str) -> Result<Option<Vec<u8>>> {
        let file = fs_path(&self.workdir, rel)?;
        match std::fs::symlink_metadata(&file) {
            Ok(md) if md.is_symlink() => {
                let target = std::fs::read_link(&file).ctx("cannot read a symbolic link")?;
                Ok(Some(
                    gix::path::into_bstr(target)
                        .ctx("invalid symbolic link")?
                        .into_owned()
                        .into(),
                ))
            }
            Ok(md) if md.is_file() => Ok(Some(std::fs::read(&file).ctx("cannot read a file")?)),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(fail(format!("cannot read {shown_as}: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_is_a_nul_byte_or_not_utf8() {
        assert!(!is_binary(b"plain text\n"));
        assert!(!is_binary("Grüße".as_bytes()));
        assert!(is_binary(b"a\0b"));
        assert!(is_binary(&[0xff, 0xfe, 0x41]));
    }

    #[test]
    fn paths_stay_inside_the_repository() {
        assert_eq!(repo_path("/a/b.yml").unwrap(), "a/b.yml");
        assert_eq!(repo_path("a.yml").unwrap(), "a.yml");
        assert!(repo_path("/../x").is_err());
        assert!(repo_path("/.git/HEAD").is_err());
        assert!(repo_path("/").is_err());
    }
}

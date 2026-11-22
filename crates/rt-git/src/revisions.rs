// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Reading any revision without a checkout (wave 12 interface): `revParse`, `treeAt`, `show`,
//! `mergeBase` and `commits` of `GitApi`. Every wave 12 strand reads history through these;
//! changing them is an interface change. (`fileLog` is in `file_log.rs`, strand D's.)

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::Mode;
use rt_app::git::{GitCommit, GitTree};

use crate::commit::describe;
use crate::repo::Repo;
use crate::util::{Context, Result, blob_bytes, fail, flatten};

impl Repo {
    /// The commit `rev` names (see `rt_app::git` for the syntax), as a full hex id.
    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        Ok(self.resolve_rev(rev)?.to_string())
    }

    pub(crate) fn resolve_rev(&self, rev: &str) -> Result<ObjectId> {
        let rev = rev.trim();
        if rev.is_empty() {
            return Err(fail("no revision given"));
        }
        let id = self
            .repo
            .rev_parse_single(rev.as_bytes().as_bstr())
            .map_err(|e| fail(format!("unknown revision {rev:?}: {e}")))?;
        let commit = id
            .object()
            .ctx("cannot read a commit")?
            .peel_to_commit()
            .map_err(|_| fail(format!("{rev:?} does not name a commit")))?;
        Ok(commit.id)
    }

    /// The collection's files at `rev` (see [`GitTree`]).
    pub fn tree_at(&self, rev: &str) -> Result<GitTree> {
        let commit = self.resolve_rev(rev)?;
        let tree = self.commit_tree(commit)?;
        let mut files = std::collections::BTreeMap::new();
        for (path, blob) in flatten(&self.repo, tree)? {
            if !matches!(blob.mode, Mode::FILE | Mode::FILE_EXECUTABLE) {
                continue;
            }
            let Some(rel) = self.scope.rel(path.as_bstr()) else {
                continue;
            };
            if !rt_app::git::tree_path(&rel) {
                continue;
            }
            if let Ok(text) = String::from_utf8(blob_bytes(&self.repo, blob.id)?) {
                files.insert(rel, text);
            }
        }
        rt_app::git::drop_nested(&mut files);
        Ok(GitTree {
            oid: commit.to_string(),
            files,
        })
    }

    /// Text of `path` at `rev`: collection-relative, or repository-relative with a leading
    /// `/`. `None` if the file does not exist there; an error if it is not UTF-8 text.
    pub fn show(&self, rev: &str, path: &str) -> Result<Option<String>> {
        let commit = self.resolve_rev(rev)?;
        let repo_path: BString = match path.strip_prefix('/') {
            Some(rest) => crate::util::Scope::new(BString::default()).abs(rest)?,
            None => self.scope.abs(path)?,
        };
        let Some((mode, id)) = self.entry_at(commit, &repo_path)? else {
            return Ok(None);
        };
        if !matches!(mode, Mode::FILE | Mode::FILE_EXECUTABLE) {
            return Ok(None);
        }
        String::from_utf8(blob_bytes(&self.repo, id)?)
            .map(Some)
            .map_err(|_| fail(format!("{path} is not a text file")))
    }

    /// Mode and object id of the repository path `path` in `commit`'s tree.
    pub(crate) fn entry_at(
        &self,
        commit: ObjectId,
        path: &BString,
    ) -> Result<Option<(Mode, ObjectId)>> {
        let tree = self
            .repo
            .find_tree(self.commit_tree(commit)?)
            .ctx("cannot read a tree")?;
        let path = path
            .to_str()
            .map_err(|_| fail("the path is not valid UTF-8"))?;
        Ok(tree
            .lookup_entry_by_path(std::path::Path::new(path))
            .ctx("cannot read a tree")?
            .map(|e| (Mode::from(e.mode()), e.object_id())))
    }

    /// The best common ancestor of `a` and `b`; `None` if they share no history.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>> {
        let (a, b) = (self.resolve_rev(a)?, self.resolve_rev(b)?);
        if a == b {
            return Ok(Some(a.to_string()));
        }
        Ok(self
            .repo
            .merge_base(a, b)
            .ctx("cannot find the merge base")?
            .map(|id| id.detach().to_string()))
    }

    /// Commits reachable from `to` along first parents, but not from `from` (`None`: down to
    /// the root), newest first, at most `limit`.
    pub fn commits(&self, from: Option<&str>, to: &str, limit: u32) -> Result<Vec<GitCommit>> {
        let to = self.resolve_rev(to)?;
        let hidden = match from {
            Some(rev) => vec![self.resolve_rev(rev)?],
            None => Vec::new(),
        };
        let walk = self
            .repo
            .rev_walk([to])
            .first_parent_only()
            .with_hidden(hidden)
            .all()
            .ctx("cannot walk the history")?;
        let mut out = Vec::new();
        for info in walk.take(limit as usize) {
            let info = info.ctx("cannot walk the history")?;
            out.push(describe(&self.repo, info.id)?);
        }
        Ok(out)
    }
}

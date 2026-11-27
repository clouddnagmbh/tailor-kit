// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The merge forecast: what merging one commit into another would do, computed entirely in
//! memory, before anything is merged (a product's "conflict radar").
//!
//! The merge is Git's own three-way merge with rename detection, the way `git merge` and
//! `git merge-tree --write-tree` do it (gitoxide's `merge_commits`; several merge bases are
//! merged into a virtual one, unrelated histories merge against the empty tree). Every object
//! it writes (merged blobs, the merged tree, a virtual merge base) goes into an object store in
//! memory: `.git` and the working tree stay byte for byte the same, and nothing is cached on
//! disk. The files that conflict are the ones `git merge-tree --write-tree --name-only` names:
//! the paths that would have conflict stages in the index.
//!
//! The product's file merger (seam N1) is not consulted: the forecast says what Git's merge
//! does. A pull's plan (`Repo::plan`) still shows the merge on the content model.

use std::collections::BTreeSet;

use gix::ObjectId;
use gix::bstr::{BStr, BString, ByteSlice};
use gix::merge::tree::{TreatAsUnresolved, apply_index_entries::RemovalMode};
use tk_git_types::actions::GitMergeFile;

use crate::repo::Repo;
use crate::util::{Context, Flat, Result, blob_bytes, fail, flatten};

/// What merging `theirs` into `ours` would do ([`Repo::merge_forecast`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeForecast {
    /// The commit of our side.
    pub ours: String,
    /// The commit of their side.
    pub theirs: String,
    /// Their best merge base; `None`: unrelated histories (merged against the empty tree).
    pub base: Option<String>,
    /// Nothing comes in: `theirs` is already part of `ours`.
    pub up_to_date: bool,
    /// `ours` could move forward to `theirs` without a merge.
    pub fast_forward: bool,
    /// Every file both sides changed since the merge base, plus every file that conflicts, in
    /// path order: hunks of their change that merge cleanly, and the conflicts (`lines a-b` of
    /// the conflict markers in the merged file, or `whole file` for a conflict without markers:
    /// binary, deleted against changed, a rename conflict).
    pub files: Vec<GitMergeFile>,
    /// Every file their side changes since the merge base (what a merge would bring in),
    /// repository-relative with a leading `/`, sorted. Renames name both paths.
    pub incoming: Vec<String>,
}

impl MergeForecast {
    /// Files with at least one conflict.
    pub fn conflicts(&self) -> usize {
        self.files
            .iter()
            .filter(|f| !f.conflicts.is_empty())
            .count()
    }
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

fn slash(path: &BStr) -> String {
    format!("/{}", path.to_str_lossy())
}

/// Paths whose entry (mode and id) differs between `a` and `b`.
fn changed<'f>(a: &'f Flat, b: &'f Flat) -> BTreeSet<&'f BString> {
    a.iter()
        .filter(|(p, blob)| b.get(*p) != Some(blob))
        .map(|(p, _)| p)
        .chain(b.keys().filter(|p| !a.contains_key(*p)))
        .collect()
}

/// A PR's head fetched for a local review ([`Repo::review_heads`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewHead {
    /// Full name: `refs/<namespace>/review/<host>/<number>`.
    pub reference: String,
    /// The host part of the name (`github`, `gitlab`, …).
    pub host: String,
    pub number: u64,
    /// The commit it points to.
    pub commit: String,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl Repo {
    /// The repository's default branch, as a full ref name: what `refs/remotes/origin/HEAD`
    /// points to, else the first of `origin/main`, `origin/master`, `main`, `master` that exists.
    /// `None`: none of them. Reads only local refs (what the last fetch brought).
    pub fn default_branch(&self) -> Result<Option<String>> {
        if let Ok(Some(r)) = self.repo.try_find_reference("refs/remotes/origin/HEAD")
            && let Some(target) = r.target().try_name()
        {
            let target = target.as_bstr().to_str_lossy().into_owned();
            if self
                .repo
                .try_find_reference(target.as_str())
                .ok()
                .flatten()
                .is_some()
            {
                return Ok(Some(target));
            }
        }
        for name in [
            "refs/remotes/origin/main",
            "refs/remotes/origin/master",
            "refs/heads/main",
            "refs/heads/master",
        ] {
            if self.repo.try_find_reference(name).ok().flatten().is_some() {
                return Ok(Some(name.to_owned()));
            }
        }
        Ok(None)
    }

    /// The PR heads fetched for a local review (seam N8: `RefNamespace::review_ref`), in this
    /// namespace and the ones it reads, sorted by name; their `-base` refs are left out.
    pub fn review_heads(&self) -> Result<Vec<ReviewHead>> {
        let mut out = Vec::new();
        let names =
            std::iter::once(self.namespace.name()).chain(self.namespace.reads().iter().copied());
        for ns in names {
            let prefix = format!("refs/{ns}/review/");
            let platform = self.repo.references().ctx("cannot list the refs")?;
            let iter = platform
                .prefixed(prefix.as_str())
                .ctx("cannot list the refs")?;
            for r in iter {
                let Ok(mut r) = r else { continue };
                let full = r.name().as_bstr().to_str_lossy().into_owned();
                let Some((host, number)) = full
                    .strip_prefix(prefix.as_str())
                    .and_then(|rest| rest.split_once('/'))
                else {
                    continue;
                };
                let Ok(number) = number.parse::<u64>() else {
                    continue;
                };
                let Ok(commit) = r.peel_to_commit() else {
                    continue;
                };
                out.push(ReviewHead {
                    host: host.to_owned(),
                    number,
                    commit: commit.id.to_string(),
                    reference: full,
                });
            }
        }
        out.sort_by(|a, b| a.reference.cmp(&b.reference));
        Ok(out)
    }

    /// What merging `theirs` into `ours` would do (any revisions naming commits), computed in an
    /// object store in memory: nothing in `.git` or the working tree changes. See
    /// [`MergeForecast`]; the module documentation says how the merge is done.
    pub fn merge_forecast(&self, ours: &str, theirs: &str) -> Result<MergeForecast> {
        let (ours_id, theirs_id) = (self.resolve_rev(ours)?, self.resolve_rev(theirs)?);
        let base = if ours_id == theirs_id {
            Some(ours_id)
        } else {
            self.repo
                .merge_base(ours_id, theirs_id)
                .ok()
                .flatten()
                .map(|id| id.detach())
        };
        let mut forecast = MergeForecast {
            ours: ours_id.to_string(),
            theirs: theirs_id.to_string(),
            base: base.map(|b| b.to_string()),
            up_to_date: base == Some(theirs_id),
            fast_forward: base == Some(ours_id) && ours_id != theirs_id,
            files: Vec::new(),
            incoming: Vec::new(),
        };
        if forecast.up_to_date {
            return Ok(forecast);
        }
        let flat = |c: ObjectId| -> Result<Flat> { flatten(&self.repo, self.commit_tree(c)?) };
        let theirs_flat = flat(theirs_id)?;
        if forecast.fast_forward {
            let ours_flat = flat(ours_id)?;
            forecast.incoming = changed(&ours_flat, &theirs_flat)
                .into_iter()
                .map(|p| slash(p.as_bstr()))
                .collect();
            return Ok(forecast);
        }

        let mut mem = self.repo.clone().with_object_memory();
        mem.object_cache_size_if_unset(32 * 1024 * 1024);
        let labels = gix::merge::blob::builtin_driver::text::Labels {
            ancestor: Some(BStr::new("base")),
            current: Some(BStr::new(ours)),
            other: Some(BStr::new(theirs)),
        };
        // Rename detection as configured (`merge.renames`, `diff.renames`; on by default), like
        // `git merge-tree`.
        let options = mem
            .tree_merge_options()
            .ctx("cannot read the merge configuration")?;
        let failed = |e: gix::Error| fail(format!("cannot merge {theirs} into {ours}: {e}"));
        let (mut outcome, base_tree) = match base {
            Some(_) => {
                let o = mem
                    .merge_commits(ours_id, theirs_id, labels, options.into())
                    .map_err(|e| failed(gix::Error::from_error(e)))?;
                (o.tree_merge, o.merge_base_tree_id)
            }
            None => {
                let empty = ObjectId::empty_tree(mem.object_hash());
                let o = mem
                    .merge_trees(
                        empty,
                        self.commit_tree(ours_id)?,
                        self.commit_tree(theirs_id)?,
                        labels,
                        options,
                    )
                    .map_err(|e| failed(gix::Error::from_error(e)))?;
                (o, empty)
            }
        };
        let merged_tree = outcome
            .tree
            .write()
            .ctx("cannot write the merged tree")?
            .detach();

        // The conflicted paths: those Git's index would hold in stages 1-3 after this merge.
        let mut index = mem
            .index_from_tree(&merged_tree)
            .ctx("cannot read the merged tree")?;
        outcome.index_changed_after_applying_conflicts(
            &mut index,
            TreatAsUnresolved::git(),
            RemovalMode::Prune,
        );
        let conflicted: BTreeSet<BString> = index
            .entries()
            .iter()
            .filter(|e| e.stage() != gix::index::entry::Stage::Unconflicted)
            .map(|e| e.path(&index).to_owned())
            .collect();
        // The merged tree itself holds the files with conflict markers.
        let marked = flatten(&mem, merged_tree)?;

        let base_flat = flatten(&mem, base_tree)?;
        let ours_flat = flat(ours_id)?;
        let theirs_changed = changed(&base_flat, &theirs_flat);
        let ours_changed = changed(&base_flat, &ours_flat);
        forecast.incoming = theirs_changed.iter().map(|p| slash(p.as_bstr())).collect();

        let mut paths: BTreeSet<&BString> = theirs_changed
            .into_iter()
            .filter(|p| ours_changed.contains(p) && ours_flat.get(*p) != theirs_flat.get(*p))
            .collect();
        paths.extend(conflicted.iter());
        let bytes = |repo: &gix::Repository, flat: &Flat, p: &BString| -> Result<Option<Vec<u8>>> {
            flat.get(p)
                .map(|blob| blob_bytes(repo, blob.id))
                .transpose()
        };
        for p in paths {
            let hunks = tk_git_types::text_diff(
                &p.to_str_lossy(),
                bytes(&mem, &base_flat, p)?.as_deref(),
                bytes(&mem, &theirs_flat, p)?.as_deref(),
            )
            .hunks
            .len();
            let hunks = count(hunks.max(1));
            let conflicts = if conflicted.contains(p) {
                let ranges = match marked.get(p) {
                    Some(blob) => marker_ranges(&blob_bytes(&mem, blob.id)?),
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
            forecast.files.push(GitMergeFile {
                path: slash(p.as_bstr()),
                semantic: false,
                clean: hunks.saturating_sub(count(conflicts.len())),
                conflicts,
            });
        }
        Ok(forecast)
    }
}

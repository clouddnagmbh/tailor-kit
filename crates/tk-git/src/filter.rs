// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The conversion between working files and blobs (seam N11): Git's filter pipeline (`eol`,
//! `ident`, `working-tree-encoding`, configured drivers) plus the product's
//! [`ContentFilter`] for the paths whose `filter` attribute it handles.
//!
//! Every place that hashes, stores or writes a working file goes through [`Filters`]. Without a
//! content filter it is gitoxide's own pipeline, unchanged. With one, the drivers of the names it
//! handles are taken out of Git's pipeline (no `git-lfs` process is started) and the filter runs
//! instead: clean before Git's conversion to the object store, smudge after the conversion to
//! the working tree, the order Git applies a driver in.

use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;

use gix::ObjectId;
use gix::bstr::{BStr, ByteSlice};
use gix::filter::plumbing::driver::apply::{Delay, MaybeDelayed};
use gix::filter::plumbing::pipeline::convert::{ToGitOutcome, ToWorktreeOutcome, to_worktree};
use gix::objs::tree::EntryKind;
use tk_git_types::filter::{ContentFilter, FilterFile};

use crate::repo::Repo;
use crate::util::{Context, Result, fail};

/// Git's filter pipeline of a repository, with the product's content filter.
pub(crate) struct Filters<'r> {
    repo: &'r gix::Repository,
    workdir: &'r Path,
    inner: Inner<'r>,
}

enum Inner<'r> {
    /// No content filter: gitoxide's pipeline as it is.
    Git(Box<gix::filter::Pipeline<'r>>),
    Content(Box<Content>),
}

struct Content {
    filter: Arc<dyn ContentFilter>,
    pipeline: gix::filter::plumbing::Pipeline,
    attributes: gix::worktree::Stack,
    /// The `filter` attribute only.
    selected: gix::attrs::search::Outcome,
}

/// One working file as Git stores it: blob id, kind, metadata.
pub(crate) type Stored = (ObjectId, EntryKind, std::fs::Metadata);

impl<'r> Filters<'r> {
    pub(crate) fn new(repo: &'r Repo) -> Result<Self> {
        Self::with_attributes_of(repo, None)
    }

    /// For a checkout: the content filter reads the attributes of `target` (the index being
    /// checked out), like Git, so a `.gitattributes` written by the same checkout counts.
    pub(crate) fn for_checkout(repo: &'r Repo, target: &gix::index::State) -> Result<Self> {
        Self::with_attributes_of(repo, Some(target))
    }

    fn with_attributes_of(repo: &'r Repo, target: Option<&gix::index::State>) -> Result<Self> {
        let (pipeline, _) = repo
            .repo
            .filter_pipeline(None)
            .ctx("cannot read the attributes")?;
        let inner = match &repo.filter {
            None => Inner::Git(Box::new(pipeline)),
            Some(filter) => {
                let (mut pipeline, mut attributes) = pipeline.into_parts();
                if let Some(target) = target {
                    attributes = repo
                        .repo
                        .attributes_only(
                            target,
                            gix::worktree::stack::state::attributes::Source::IdMapping,
                        )
                        .ctx("cannot read the attributes")?
                        .detach();
                }
                pipeline
                    .options_mut()
                    .drivers
                    .retain(|d| !filter.handles(&d.name.to_str_lossy()));
                let selected = attributes.selected_attribute_matches(["filter"]);
                Inner::Content(Box::new(Content {
                    filter: filter.clone(),
                    pipeline,
                    attributes,
                    selected,
                }))
            }
        };
        Ok(Self {
            repo: &repo.repo,
            workdir: &repo.workdir,
            inner,
        })
    }

    /// Whether the content filter takes `path` (its `filter` attribute value), `None` without a
    /// content filter or when the attribute names another filter.
    pub(crate) fn content_name(&mut self, path: &BStr) -> Result<Option<String>> {
        let Inner::Content(c) = &mut self.inner else {
            return Ok(None);
        };
        c.name(self.repo, path)
    }

    fn file<'a>(&'a self, name: &'a str, path: &'a str) -> FilterFile<'a> {
        FilterFile {
            workdir: self.workdir,
            common_dir: self.repo.common_dir(),
            name,
            path,
        }
    }

    /// Working-tree bytes of `path` (a repository path) → what Git stores.
    pub(crate) fn convert_to_git(
        &mut self,
        path: &BStr,
        bytes: Vec<u8>,
        index: &gix::index::State,
    ) -> Result<Vec<u8>> {
        let rel = gix::path::from_bstr(path).map_err(|_| fail(format!("invalid path {path}")))?;
        let repo = self.repo;
        let workdir = self.workdir;
        match &mut self.inner {
            Inner::Git(pipeline) => {
                let out = pipeline
                    .convert_to_git(&bytes[..], &rel, index)
                    .ctx("cannot convert a file")?;
                read_to_git(out, &bytes)
            }
            Inner::Content(c) => {
                let bytes = match c.name(repo, path)? {
                    Some(name) => {
                        let shown = path.to_str_lossy();
                        let file = FilterFile {
                            workdir,
                            common_dir: repo.common_dir(),
                            name: &name,
                            path: &shown,
                        };
                        c.filter.clean(&file, bytes)?
                    }
                    None => bytes,
                };
                let Content {
                    pipeline,
                    attributes,
                    ..
                } = &mut **c;
                let entry = attributes
                    .at_entry(path, None, &repo.objects)
                    .ctx("cannot read the attributes")?;
                let out = pipeline
                    .convert_to_git(
                        &bytes[..],
                        &rel,
                        &mut |_, attrs| {
                            entry.matching_attributes(attrs);
                        },
                        &mut |buf| Ok(index_blob(repo, index, path, buf)),
                    )
                    .ctx("cannot convert a file")?;
                read_to_git(out, &bytes)
            }
        }
    }

    /// What Git stores for `path` → the working-tree bytes.
    pub(crate) fn convert_to_worktree(&mut self, path: &BStr, data: &[u8]) -> Result<Vec<u8>> {
        let options = to_worktree::Options {
            can_delay: Delay::Forbid,
            unknown_encoding: Default::default(),
        };
        let repo = self.repo;
        let workdir = self.workdir;
        match &mut self.inner {
            Inner::Git(pipeline) => {
                let out = pipeline
                    .convert_to_worktree(data, path, options)
                    .ctx("cannot convert a file")?;
                read_to_worktree(out, path)
            }
            Inner::Content(c) => {
                let name = c.name(repo, path)?;
                let converted = {
                    let Content {
                        pipeline,
                        attributes,
                        ..
                    } = &mut **c;
                    let entry = attributes
                        .at_entry(path, None, &repo.objects)
                        .ctx("cannot read the attributes")?;
                    let out = pipeline
                        .convert_to_worktree(
                            data,
                            path,
                            &mut |_, attrs| {
                                entry.matching_attributes(attrs);
                            },
                            options,
                        )
                        .ctx("cannot convert a file")?;
                    read_to_worktree(out, path)?
                };
                match name {
                    Some(name) => {
                        let shown = path.to_str_lossy();
                        let file = FilterFile {
                            workdir,
                            common_dir: repo.common_dir(),
                            name: &name,
                            path: &shown,
                        };
                        c.filter.smudge(&file, converted)
                    }
                    None => Ok(converted),
                }
            }
        }
    }

    /// Lets the content filter prepare a checkout of `files` (repository path, stored bytes):
    /// only those it handles reach it, and only if there are any.
    pub(crate) fn prepare_smudge(&mut self, files: &[(&BStr, Vec<u8>)]) -> Result<()> {
        let repo = self.repo;
        let Inner::Content(c) = &mut self.inner else {
            return Ok(());
        };
        let mut named = Vec::new();
        for (path, data) in files {
            if let Some(name) = c.name(repo, path)? {
                named.push((name, path.to_str_lossy().into_owned(), data.as_slice()));
            }
        }
        if named.is_empty() {
            return Ok(());
        }
        let filter = c.filter.clone();
        let list: Vec<(FilterFile<'_>, &[u8])> = named
            .iter()
            .map(|(name, path, data)| (self.file(name, path), *data))
            .collect();
        filter.prepare_smudge(&list)
    }

    /// Stores the working file at `path` as Git would add it: its blob (written), kind and
    /// metadata. A folder holding a repository is that repository's `HEAD` commit; `None` for
    /// a missing file or one Git cannot track.
    pub(crate) fn worktree_file_to_object(
        &mut self,
        path: &BStr,
        index: &gix::index::State,
    ) -> Result<Option<Stored>> {
        if let Inner::Git(pipeline) = &mut self.inner {
            return pipeline
                .worktree_file_to_object(path, index)
                .ctx("cannot read a file");
        }
        let file = crate::worktree::fs_path(self.workdir, path)?;
        let md = match std::fs::symlink_metadata(&file) {
            Ok(md) => md,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(fail(format!("cannot read {path}: {e}"))),
        };
        let (id, kind) = if md.is_symlink() {
            let target = std::fs::read_link(&file).ctx("cannot read a symbolic link")?;
            let bytes = gix::path::into_bstr(target)
                .ctx("invalid symbolic link")?
                .into_owned();
            let id = self.repo.write_blob(&bytes).ctx("cannot write a file")?;
            (id.detach(), EntryKind::Link)
        } else if md.is_file() {
            let bytes =
                std::fs::read(&file).map_err(|e| fail(format!("cannot read {path}: {e}")))?;
            let data = self.convert_to_git(path, bytes, index)?;
            let id = self.repo.write_blob(&data).ctx("cannot write a file")?;
            let kind = if gix::fs::is_executable(&md) {
                EntryKind::BlobExecutable
            } else {
                EntryKind::Blob
            };
            (id.detach(), kind)
        } else if md.is_dir() {
            let Ok(sub) = gix::open_opts(&file, self.repo.open_options().clone()) else {
                return Ok(None);
            };
            let Ok(head) = sub.head_id() else {
                return Ok(None);
            };
            (head.detach(), EntryKind::Commit)
        } else {
            return Ok(None);
        };
        Ok(Some((id, kind, md)))
    }
}

impl Content {
    fn name(&mut self, repo: &gix::Repository, path: &BStr) -> Result<Option<String>> {
        self.selected.reset();
        self.attributes
            .at_entry(path, None, &repo.objects)
            .ctx("cannot read the attributes")?
            .matching_attributes(&mut self.selected);
        let name = self
            .selected
            .iter_selected()
            .find_map(|m| match m.assignment.state {
                gix::attrs::StateRef::Value(v) => Some(v.as_bstr().to_str_lossy().into_owned()),
                _ => None,
            });
        Ok(name.filter(|n| self.filter.handles(n)))
    }
}

/// The blob the index holds for `path` (for `crlf=auto`, which looks at it), into `buf`.
fn index_blob(
    repo: &gix::Repository,
    index: &gix::index::State,
    path: &BStr,
    buf: &mut Vec<u8>,
) -> Option<()> {
    let entry = index.entry_by_path(path)?;
    let object = repo.find_object(entry.id).ok()?;
    if object.kind != gix::objs::Kind::Blob {
        return None;
    }
    buf.clear();
    buf.extend_from_slice(&object.data);
    Some(())
}

fn read_to_git<R: std::io::Read>(out: ToGitOutcome<'_, R>, bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(match out {
        ToGitOutcome::Unchanged(_) => bytes.to_vec(),
        ToGitOutcome::Buffer(buf) => buf.to_vec(),
        ToGitOutcome::Process(mut read) => {
            let mut out = Vec::new();
            read.read_to_end(&mut out).ctx("cannot convert a file")?;
            out
        }
    })
}

fn read_to_worktree(out: ToWorktreeOutcome<'_, '_>, path: &BStr) -> Result<Vec<u8>> {
    Ok(match out {
        ToWorktreeOutcome::Unchanged(b) => b.to_vec(),
        ToWorktreeOutcome::Buffer(b) => b.to_vec(),
        ToWorktreeOutcome::Process(MaybeDelayed::Immediate(mut read)) => {
            let mut out = Vec::new();
            read.read_to_end(&mut out).ctx("cannot convert a file")?;
            out
        }
        ToWorktreeOutcome::Process(MaybeDelayed::Delayed(_)) => {
            return Err(fail(format!("a filter delayed {path}")));
        }
    })
}

impl Repo {
    /// `data` as stored for `path` (a repository path) → its working-tree bytes, when the
    /// content filter takes `path`; otherwise unchanged (the viewers show Git's bytes).
    pub(crate) fn smudged(&self, path: &BStr, data: Vec<u8>) -> Result<Vec<u8>> {
        if self.filter.is_none() {
            return Ok(data);
        }
        let mut filters = Filters::new(self)?;
        if filters.content_name(path)?.is_none() {
            return Ok(data);
        }
        filters.convert_to_worktree(path, &data)
    }

    /// The working-tree bytes of `path` → what Git would store, when the content filter takes
    /// `path`; otherwise unchanged (diffs compare what Git compares).
    pub(crate) fn cleaned(&self, path: &BStr, data: Vec<u8>) -> Result<Vec<u8>> {
        if self.filter.is_none() {
            return Ok(data);
        }
        let mut filters = Filters::new(self)?;
        if filters.content_name(path)?.is_none() {
            return Ok(data);
        }
        let index = self.load_index()?;
        filters.convert_to_git(path, data, &index)
    }
}

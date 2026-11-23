// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Upstreams, fetch, push, pull and clone.

use std::path::{Path, PathBuf};

use gix::ObjectId;
use gix::refs::transaction::PreviousValue;
use rt_app::git::{GitIdentity, GitPullResult};

use crate::branch::branch_ref;
use crate::credentials::CredentialSource;
use crate::local::is_ancestor;
use crate::net::{Endpoint, endpoint};
use crate::repo::{Options, Repo, set_local_config};
use crate::util::{Context, Result, fail};
use crate::worktree::switch;

/// The branch the current branch pulls from and pushes to.
pub(crate) struct Upstream {
    pub remote: String,
    /// On the remote, e.g. `refs/heads/main`.
    pub merge: String,
    /// Local remote-tracking ref, e.g. `refs/remotes/origin/main`.
    pub tracking: String,
    /// `origin/main`.
    pub display: String,
}

impl Upstream {
    fn new(remote: &str, merge: &str) -> Option<Self> {
        let short = merge.strip_prefix("refs/heads/")?;
        Some(Self {
            remote: remote.to_owned(),
            merge: merge.to_owned(),
            tracking: format!("refs/remotes/{remote}/{short}"),
            display: format!("{remote}/{short}"),
        })
    }
}

impl Repo {
    /// The configured upstream of the current branch.
    pub(crate) fn upstream(&self) -> Result<Option<Upstream>> {
        let Some(branch) = self.head_branch()? else {
            return Ok(None);
        };
        let (Some(remote), Some(merge)) = (
            self.config(&format!("branch.{branch}.remote")),
            self.config(&format!("branch.{branch}.merge")),
        ) else {
            return Ok(None);
        };
        if remote == "." {
            return Ok(None);
        }
        Ok(Upstream::new(&remote, &merge))
    }

    fn remote_endpoint(&self, remote: &str, push: bool) -> Result<Endpoint> {
        let url = push
            .then(|| self.config(&format!("remote.{remote}.pushurl")))
            .flatten()
            .or_else(|| self.config(&format!("remote.{remote}.url")))
            .ok_or_else(|| fail(format!("no remote named {remote}: add one first")))?;
        endpoint(&url, &self.workdir)
    }

    /// Fetches `remote` into its remote-tracking branches; returns the remote's `HEAD` branch
    /// if it said.
    pub(crate) fn fetch(
        &self,
        remote: &str,
        creds: &dyn CredentialSource,
        with_head: bool,
    ) -> Result<Option<String>> {
        match self.remote_endpoint(remote, false)? {
            Endpoint::Local(path) => crate::local::fetch(&self.repo, remote, &path, &self.options),
            Endpoint::Http(target) => {
                crate::net::fetch(&self.repo, remote, &target, creds, with_head)
            }
        }
    }

    /// Sets `remote_ref` on `remote` to `new` (a commit, or an annotated tag object), sending
    /// what the remote lacks. Fast-forward only for branches; a ref that exists with another
    /// value is refused for tags.
    pub(crate) fn push_ref(
        &self,
        remote: &str,
        new: ObjectId,
        remote_ref: &str,
        creds: &dyn CredentialSource,
    ) -> Result<()> {
        match self.remote_endpoint(remote, true)? {
            Endpoint::Local(path) => {
                crate::local::push(&self.repo, &path, &self.options, new, remote_ref)
            }
            Endpoint::Http(target) => {
                crate::http::push(&self.repo, &target, creds, new, remote_ref)
            }
        }
    }

    /// What `remote_ref` is on `remote` when that can be known without the network (a
    /// repository on this machine): `Some(None)` it does not exist there, `None` unknown.
    pub(crate) fn remote_ref_offline(
        &self,
        remote: &str,
        remote_ref: &str,
    ) -> Option<Option<ObjectId>> {
        let Ok(Endpoint::Local(path)) = self.remote_endpoint(remote, true) else {
            return None;
        };
        let other = crate::local::open(&path, &self.options).ok()?;
        let found = other.try_find_reference(remote_ref).ok()?;
        Some(found.and_then(|r| match r.target() {
            gix::refs::TargetRef::Object(id) => Some(id.to_owned()),
            gix::refs::TargetRef::Symbolic(_) => None,
        }))
    }

    /// Pushes the current branch to its upstream; without one to `origin/<branch>`, which
    /// then becomes the upstream. Fast-forward only.
    pub fn push(&self, creds: &dyn CredentialSource) -> Result<()> {
        let branch = self
            .head_branch()?
            .ok_or_else(|| fail("not on a branch: switch to one before pushing"))?;
        let head = self
            .head_commit()?
            .ok_or_else(|| fail("nothing to push: commit something first"))?;
        if self.underway()?.is_some() {
            return Err(fail("finish the merge (resolve and commit) before pushing"));
        }
        let (upstream, new_upstream) = match self.upstream()? {
            Some(up) => (up, false),
            None => (
                Upstream::new("origin", &format!("refs/heads/{branch}"))
                    .ok_or_else(|| fail("invalid branch"))?,
                true,
            ),
        };
        match self.remote_endpoint(&upstream.remote, true)? {
            Endpoint::Local(path) => {
                crate::local::push(&self.repo, &path, &self.options, head, &upstream.merge)?;
            }
            Endpoint::Http(target) => {
                crate::http::push(&self.repo, &target, creds, head, &upstream.merge)?;
            }
        }
        self.repo
            .reference(
                upstream.tracking.as_str(),
                head,
                PreviousValue::Any,
                "update by push",
            )
            .ctx("cannot update the remote-tracking branch")?;
        if new_upstream {
            self.set_local_config(&[
                ("branch", &branch, "remote", &upstream.remote),
                ("branch", &branch, "merge", &upstream.merge),
            ])?;
        }
        Ok(())
    }

    /// Fetches the upstream and brings the current branch up to date with it: nothing to do,
    /// fast-forward, a merge commit, or a stopped merge with conflicts. Without a configured
    /// upstream, `origin/<branch>` is used (and becomes the upstream).
    pub fn pull(
        &self,
        creds: &dyn CredentialSource,
        fallback: Option<&GitIdentity>,
    ) -> Result<GitPullResult> {
        if self.underway()?.is_some() {
            return Err(fail(
                "finish the merge (resolve and commit) before pulling again",
            ));
        }
        let branch = self
            .head_branch()?
            .ok_or_else(|| fail("not on a branch: switch to one before pulling"))?;
        let (upstream, new_upstream) = match self.upstream()? {
            Some(up) => (up, false),
            None => (
                Upstream::new("origin", &format!("refs/heads/{branch}"))
                    .ok_or_else(|| fail("invalid branch"))?,
                true,
            ),
        };
        self.fetch(&upstream.remote, creds, false)?;
        let theirs = self
            .ref_commit(&upstream.tracking)?
            .ok_or_else(|| fail(format!("the remote has no branch {}", upstream.display)))?;
        if new_upstream {
            self.set_local_config(&[
                ("branch", &branch, "remote", &upstream.remote),
                ("branch", &branch, "merge", &upstream.merge),
            ])?;
        }
        let local = branch_ref(&branch)?;
        let Some(ours) = self.head_commit()? else {
            // Nothing committed here yet: take the remote branch as it is.
            let mut index = switch(self, self.head_tree()?, self.commit_tree(theirs)?, "pull")?;
            self.write_index(&mut index)?;
            self.set_branch(
                &local,
                theirs,
                PreviousValue::MustNotExist,
                "pull: fast-forward",
            )?;
            let commits = self.count_between(theirs, None)?;
            return Ok(GitPullResult::FastForward { commits });
        };
        if is_ancestor(&self.repo, theirs, ours) {
            return Ok(GitPullResult::UpToDate);
        }
        if is_ancestor(&self.repo, ours, theirs) {
            let mut index = switch(
                self,
                self.commit_tree(ours)?,
                self.commit_tree(theirs)?,
                "pull",
            )?;
            self.write_index(&mut index)?;
            self.set_branch(
                &local,
                theirs,
                PreviousValue::ExistingMustMatch(gix::refs::Target::Object(ours)),
                "pull: fast-forward",
            )?;
            let commits = self.count_between(theirs, Some(ours))?;
            return Ok(GitPullResult::FastForward { commits });
        }
        let identity = self.identity_or(fallback)?;
        self.merge(ours, theirs, &upstream.display, &identity)
    }

    /// Commits reachable from `tip` but not from `hidden`.
    fn count_between(&self, tip: ObjectId, hidden: Option<ObjectId>) -> Result<u32> {
        let walk = self
            .repo
            .rev_walk([tip])
            .with_hidden(hidden)
            .all()
            .ctx("cannot walk the history")?;
        let mut n = 0;
        for info in walk {
            info.ctx("cannot walk the history")?;
            n += 1;
        }
        Ok(n)
    }
}

/// Folder name for a clone of `url`: its last path segment without `.git`.
pub fn repo_name(url: &str) -> Result<String> {
    let trimmed = url.trim().trim_end_matches(['/', '\\']);
    let last = trimmed.rsplit(['/', '\\', ':']).next().unwrap_or_default();
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() || name == "." || name == ".." || name.starts_with('.') {
        return Err(fail(format!("cannot derive a folder name from {url:?}")));
    }
    Ok(name.to_owned())
}

/// The user's home folder (`HOME`, on Windows `USERPROFILE`).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|h| !h.is_empty()))
        .map(PathBuf::from)
}

/// Expands a leading `~` or `~/` (also `file://~/…`) against `home`; anything else is
/// returned unchanged.
pub fn expand_home(url: &str, home: Option<&Path>) -> Result<String> {
    let (prefix, rest) = match url.strip_prefix("file://") {
        Some(r) if r.starts_with('~') => ("file://", r),
        _ => ("", url),
    };
    let tail = if rest == "~" {
        ""
    } else if let Some(t) = rest.strip_prefix("~/").or_else(|| rest.strip_prefix("~\\")) {
        t
    } else {
        return Ok(url.to_owned());
    };
    let home = home.ok_or_else(|| fail("cannot find your home folder to expand \"~\""))?;
    let full = if tail.is_empty() {
        home.to_path_buf()
    } else {
        home.join(tail)
    }
    .to_string_lossy()
    .replace('\\', "/");
    Ok(if prefix.is_empty() {
        full
    } else {
        format!("{prefix}{full}")
    })
}

/// Whether `url` names a remote on the network rather than a path on this machine.
fn is_network(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("file://") {
        return false;
    }
    if lower.contains("://") {
        return true;
    }
    // scp-like `git@host:path`: a colon before any slash, not a Windows drive (`C:\`).
    match (url.find(':'), url.find(['/', '\\'])) {
        (Some(c), slash) => c > 1 && slash.is_none_or(|s| c < s),
        _ => false,
    }
}

/// Clones `url` into `<parent>/<repository name>` and checks out the remote's default
/// branch. `url` is an HTTPS URL or a repository on this machine: an absolute path, `~/…`,
/// `file://…`, or a relative path (to `parent`, else to the home folder); a bare repository
/// or the folder of a normal one. Refuses if the target folder exists and is not empty.
/// Returns the new folder.
pub fn clone(
    url: &str,
    parent: &Path,
    creds: &dyn CredentialSource,
    options: &Options,
) -> Result<PathBuf> {
    clone_with_home(url, parent, home_dir().as_deref(), creds, options)
}

/// [`clone`] with an explicit home folder (for `~` and relative paths).
pub fn clone_with_home(
    url: &str,
    parent: &Path,
    home: Option<&Path>,
    creds: &dyn CredentialSource,
    options: &Options,
) -> Result<PathBuf> {
    let url = url.trim();
    if url.is_empty() {
        return Err(fail("enter the address or the folder of a repository"));
    }
    let parent = crate::util::canonical(parent).ctx("cannot open the folder")?;
    // Validate the source before creating anything.
    let (url, name) = if is_network(url) {
        (url.to_owned(), repo_name(url)?)
    } else {
        let expanded = expand_home(url, home)?;
        let Endpoint::Local(path) = endpoint(&expanded, &parent)? else {
            return Err(fail(format!("cannot read {url:?} as a repository")));
        };
        let mut found = crate::util::canonical(&path).ok();
        if found.is_none()
            && !Path::new(&expanded).is_absolute()
            && !url.starts_with("file://")
            && let Some(home) = home
        {
            found = crate::util::canonical(&home.join(&expanded)).ok();
        }
        let Some(path) = found else {
            return Err(fail(format!(
                "the repository {} does not exist",
                path.display()
            )));
        };
        // Must be a Git repository: bare, or a working tree.
        crate::local::open(&path, options)
            .map_err(|_| fail(format!("{} is not a Git repository", path.display())))?;
        let named = if path.file_name().is_some_and(|n| n == ".git") {
            path.parent().unwrap_or(&path)
        } else {
            &path
        };
        let name = repo_name(&named.to_string_lossy())?;
        // Forward slashes: Git configuration treats backslashes as escapes.
        (path.to_string_lossy().replace('\\', "/"), name)
    };
    let target = parent.join(&name);
    let existed = target.exists();
    if existed {
        let empty = std::fs::read_dir(&target)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Err(fail(format!(
                "{} already exists and is not empty",
                target.display()
            )));
        }
    }
    std::fs::create_dir_all(&target).ctx("cannot create the folder")?;
    let result = clone_into(&url, &target, creds, options);
    if result.is_err() {
        // Leave nothing half-cloned behind.
        let _ = std::fs::remove_dir_all(&target);
        if existed {
            let _ = std::fs::create_dir(&target);
        }
    }
    result.map(|()| target)
}

fn clone_into(
    url: &str,
    target: &Path,
    creds: &dyn CredentialSource,
    options: &Options,
) -> Result<()> {
    let repo = gix::ThreadSafeRepository::init_opts(
        target,
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        options.open(),
    )
    .ctx("cannot create the repository")?;
    let git_dir = repo.git_dir().to_path_buf();
    set_local_config(
        &git_dir,
        &[
            ("remote", "origin", "url", url),
            (
                "remote",
                "origin",
                "fetch",
                "+refs/heads/*:refs/remotes/origin/*",
            ),
        ],
    )?;
    let repo = Repo::require(target, options)?;
    let head = repo.fetch("origin", creds, true)?;
    let candidates = head
        .into_iter()
        .chain(["main".to_owned(), "master".to_owned()]);
    let mut chosen = None;
    for b in candidates {
        if let Some(id) = repo.ref_commit(&format!("refs/remotes/origin/{b}"))? {
            chosen = Some((b, id));
            break;
        }
    }
    if chosen.is_none() {
        let refs = repo.repo.references().ctx("cannot list branches")?;
        for r in refs.remote_branches().ctx("cannot list branches")? {
            let mut r = r.map_err(|e| fail(format!("cannot list branches: {e}")))?;
            let name = r.name().as_bstr().to_string();
            if let Some(b) = name.strip_prefix("refs/remotes/origin/")
                && b != "HEAD"
            {
                let id = r.peel_to_id().ctx("cannot read a branch")?.detach();
                chosen = Some((b.to_owned(), id));
                break;
            }
        }
    }
    let Some((branch, id)) = chosen else {
        // An empty remote: an empty clone, ready for the first commit.
        return Ok(());
    };
    let local = branch_ref(&branch)?;
    let mut index = switch(&repo, repo.head_tree()?, repo.commit_tree(id)?, "clone")?;
    repo.write_index(&mut index)?;
    repo.set_branch(&local, id, PreviousValue::Any, "clone")?;
    let merge = format!("refs/heads/{branch}");
    repo.set_local_config(&[
        ("branch", &branch, "remote", "origin"),
        ("branch", &branch, "merge", &merge),
    ])?;
    repo.reopen()?.set_head(&local, "clone")
}

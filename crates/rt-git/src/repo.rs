// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Opening the repository of a collection, and the small pieces every operation needs.

use std::path::{Path, PathBuf};

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use rt_app::git::GitIdentity;

use crate::util::{Context, Result, Scope, canonical, fail};

/// A merge, cherry-pick or revert that stopped on conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Underway {
    Merge,
    CherryPick,
    Revert,
}

/// How repositories are opened.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Read only the repository's own configuration: no global or system Git configuration,
    /// no `GIT_*` environment variables. For tests; the app reads the user's configuration.
    pub isolated: bool,
}

impl Options {
    pub(crate) fn open(&self) -> gix::open::Options {
        let base = if self.isolated {
            gix::open::Options::isolated()
        } else {
            gix::open::Options::default()
        };
        // Reflog entries need a committer even when none is configured (a fresh clone).
        // Commits never use these: they get an explicit identity (`Repo::identity_or`).
        base.config_overrides([
            "gitoxide.committer.nameFallback=RequestTailor",
            "gitoxide.committer.emailFallback=requesttailor@localhost",
        ])
    }
}

/// The repository that contains a collection.
pub struct Repo {
    pub(crate) repo: gix::Repository,
    /// Canonical work tree root.
    pub(crate) workdir: PathBuf,
    /// Canonical collection root.
    pub(crate) root: PathBuf,
    pub(crate) scope: Scope,
    pub(crate) options: Options,
    /// What commits are signed and verified with (wave 15); the default signs nothing.
    pub(crate) signing: crate::sign::Signing,
    /// Where the app's own refs and files live (seam N8); RequestTailor's by default.
    pub(crate) namespace: tk_git_types::RefNamespace,
    /// What merges files on their model (seam N1); RequestTailor's collections by default,
    /// `None`: Git's line merge for every file.
    pub(crate) merger: Option<std::sync::Arc<dyn tk_git_types::FileMerger>>,
}

/// The folder at or above `start` that holds a `.git` (folder, or file for linked work trees).
fn find_workdir(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

impl Repo {
    /// The repository containing the collection folder `root`; `None` if there is none.
    pub fn open(root: &Path, options: &Options) -> Result<Option<Self>> {
        let root = canonical(root).ctx("cannot open the collection folder")?;
        let Some(workdir) = find_workdir(&root) else {
            return Ok(None);
        };
        let repo =
            gix::open_opts(&workdir, options.open()).ctx("cannot open the Git repository")?;
        if repo.is_bare() {
            return Ok(None);
        }
        let workdir = crate::util::canonical(repo.workdir().unwrap_or(&workdir))
            .ctx("cannot open the Git repository")?;
        let rel = root
            .strip_prefix(&workdir)
            .map_err(|_| fail("the collection is not inside the repository's work tree"))?;
        let mut prefix = BString::default();
        for part in rel.components() {
            let part = part
                .as_os_str()
                .to_str()
                .ok_or_else(|| fail("the collection path is not valid UTF-8"))?;
            if !prefix.is_empty() {
                prefix.push(b'/');
            }
            prefix.extend_from_slice(part.as_bytes());
        }
        Ok(Some(Self {
            repo,
            workdir,
            root,
            scope: Scope::new(prefix),
            options: options.clone(),
            signing: crate::sign::Signing::default(),
            namespace: rt_app::git::REF_NAMESPACE,
            merger: Some(std::sync::Arc::new(crate::CollectionMerger)),
        }))
    }

    /// Like [`Repo::open`], but "not a repository" is an error.
    pub fn require(root: &Path, options: &Options) -> Result<Self> {
        Self::open(root, options)?.ok_or_else(|| fail("the collection is not in a Git repository"))
    }

    /// The same repository opened again (after its configuration changed on disk).
    pub(crate) fn reopen(&self) -> Result<Self> {
        Ok(Self::require(&self.root, &self.options)?.with_signing(self.signing.clone()))
    }

    /// Signs and verifies commits with `signing` (a distribution's hook, `sign`); without it
    /// every commit stays unsigned.
    pub fn with_signing(mut self, signing: crate::sign::Signing) -> Self {
        self.signing = signing;
        self
    }

    /// Keeps backups, review refs and the action journal in `namespace` (seam N8) instead of
    /// RequestTailor's.
    pub fn with_namespace(mut self, namespace: tk_git_types::RefNamespace) -> Self {
        self.namespace = namespace;
        self
    }

    /// Merges files with `merger` (seam N1) instead of RequestTailor's collection merger;
    /// `None` merges every file line by line, like Git.
    pub fn with_merger(
        mut self,
        merger: Option<std::sync::Arc<dyn tk_git_types::FileMerger>>,
    ) -> Self {
        self.merger = merger;
        self
    }

    /// Where the app's own refs and files live.
    pub fn namespace(&self) -> tk_git_types::RefNamespace {
        self.namespace
    }

    /// Canonical root of the collection.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Canonical root of the work tree (may be above [`Repo::root`]).
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    pub(crate) fn git_dir(&self) -> &Path {
        self.repo.git_dir()
    }

    /// Full name of the branch `HEAD` points to, also when it has no commits yet; `None` when
    /// detached.
    pub(crate) fn head_ref(&self) -> Result<Option<gix::refs::FullName>> {
        self.repo.head_name().ctx("cannot read HEAD")
    }

    /// Short name of the current branch, also when it has no commits yet.
    pub(crate) fn head_branch(&self) -> Result<Option<String>> {
        Ok(self.head_ref()?.and_then(|name| {
            name.as_bstr()
                .strip_prefix(b"refs/heads/")
                .and_then(|s| s.to_str().ok())
                .map(str::to_owned)
        }))
    }

    /// The commit `HEAD` points to; `None` in a repository without commits.
    pub(crate) fn head_commit(&self) -> Result<Option<ObjectId>> {
        let head = self.repo.head().ctx("cannot read HEAD")?;
        if head.is_unborn() {
            return Ok(None);
        }
        let mut head = head;
        let id = head.peel_to_commit().ctx("cannot read the HEAD commit")?.id;
        Ok(Some(id))
    }

    /// Tree of `commit`.
    pub(crate) fn commit_tree(&self, commit: ObjectId) -> Result<ObjectId> {
        let commit = self.repo.find_commit(commit).ctx("cannot read a commit")?;
        Ok(commit.tree_id().ctx("cannot read a commit")?.detach())
    }

    /// Tree of `HEAD`, the empty tree in a repository without commits.
    pub(crate) fn head_tree(&self) -> Result<ObjectId> {
        match self.head_commit()? {
            Some(c) => self.commit_tree(c),
            None => Ok(ObjectId::empty_tree(self.repo.object_hash())),
        }
    }

    /// The commit a local or remote-tracking ref points to.
    pub(crate) fn ref_commit(&self, name: &str) -> Result<Option<ObjectId>> {
        let Some(mut r) = self
            .repo
            .try_find_reference(name)
            .ctx("cannot read a branch")?
        else {
            return Ok(None);
        };
        Ok(Some(r.peel_to_commit().ctx("cannot read a branch")?.id))
    }

    /// The index (empty if there is none yet), owned so it can be changed and written.
    pub(crate) fn load_index(&self) -> Result<gix::index::File> {
        let path = self.repo.index_path();
        if path.is_file() {
            self.repo.open_index().ctx("cannot read the index")
        } else {
            let empty = ObjectId::empty_tree(self.repo.object_hash());
            let mut file = self
                .repo
                .index_from_tree(&empty)
                .ctx("cannot create an index")?;
            file.set_path(path);
            Ok(file)
        }
    }

    pub(crate) fn write_index(&self, index: &mut gix::index::File) -> Result<()> {
        // The cached trees are not kept up to date by our edits.
        index.remove_tree();
        index.remove_resolve_undo();
        index.sort_entries();
        index
            .write(gix::index::write::Options::default())
            .ctx("cannot write the index")
    }

    /// `MERGE_HEAD` while a merge is in progress.
    pub(crate) fn merge_head(&self) -> Result<Option<ObjectId>> {
        match std::fs::read_to_string(self.git_dir().join("MERGE_HEAD")) {
            Ok(text) => {
                let first = text.lines().next().unwrap_or_default().trim();
                Ok(Some(
                    ObjectId::from_hex(first.as_bytes()).ctx("MERGE_HEAD is invalid")?,
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(fail(format!("cannot read MERGE_HEAD: {e}"))),
        }
    }

    /// What is half done: a merge (`MERGE_HEAD`), a cherry-pick (`CHERRY_PICK_HEAD`) or a
    /// revert (`REVERT_HEAD`) that stopped on conflicts. The two latter conclude with a commit
    /// that has one parent.
    pub(crate) fn underway(&self) -> Result<Option<Underway>> {
        if self.merge_head()?.is_some() {
            return Ok(Some(Underway::Merge));
        }
        for (file, kind) in [
            ("CHERRY_PICK_HEAD", Underway::CherryPick),
            ("REVERT_HEAD", Underway::Revert),
        ] {
            if self.git_dir().join(file).is_file() {
                return Ok(Some(kind));
            }
        }
        Ok(None)
    }

    /// A repository path from a path of the API: collection-relative, or repository-relative
    /// with a leading `/` (the wave-12 convention).
    pub(crate) fn repo_path(&self, path: &str) -> Result<BString> {
        match path.strip_prefix('/') {
            Some(rest) => Scope::new(BString::default()).abs(rest),
            None => self.scope.abs(path),
        }
    }

    /// The API form of a repository path: collection-relative inside the collection,
    /// repository-relative with a leading `/` outside it.
    pub(crate) fn api_path(&self, path: &gix::bstr::BStr) -> String {
        self.scope
            .rel(path)
            .unwrap_or_else(|| format!("/{}", path.to_str_lossy()))
    }

    /// The same repository seen from its work tree root: no collection scope, every file.
    /// Git actions (wave 14) work on the whole repository.
    pub(crate) fn whole(&self) -> Result<Self> {
        let repo = gix::open_opts(&self.workdir, self.options.open())
            .ctx("cannot open the Git repository")?;
        Ok(Self {
            repo,
            workdir: self.workdir.clone(),
            root: self.workdir.clone(),
            scope: Scope::new(BString::default()),
            options: self.options.clone(),
            signing: self.signing.clone(),
            namespace: self.namespace,
            merger: self.merger.clone(),
        })
    }

    /// The text of the repository's own `.git/config` (empty if there is none).
    pub(crate) fn config_text(&self) -> Result<String> {
        match std::fs::read_to_string(self.repo.common_dir().join("config")) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(fail(format!(
                "cannot read the repository configuration: {e}"
            ))),
        }
    }

    /// Writes `.git/config` back as `text` (undo of a configuration change).
    pub(crate) fn write_config_text(&self, text: &str) -> Result<()> {
        let dir = self.repo.common_dir();
        let tmp = dir.join("config.rt-tmp");
        std::fs::write(&tmp, text).ctx("cannot write the repository configuration")?;
        std::fs::rename(&tmp, dir.join("config")).ctx("cannot write the repository configuration")
    }

    pub(crate) fn clear_merge_state(&self) -> Result<()> {
        for name in [
            "MERGE_HEAD",
            "MERGE_MSG",
            "MERGE_MODE",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
        ] {
            match std::fs::remove_file(self.git_dir().join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(fail(format!("cannot clear the merge state: {e}"))),
            }
        }
        Ok(())
    }

    /// A configuration value (repository, then global and system, unless isolated).
    pub(crate) fn config(&self, key: &str) -> Option<String> {
        self.repo
            .config_snapshot()
            .string(key)
            .and_then(|v| v.to_str().ok().map(str::to_owned))
    }

    /// An effective Git configuration value (`GitApi.config`): the repository's own
    /// configuration first, then the user's global and system configuration (not when the
    /// repository was opened `isolated`). `None` if the key is not set.
    pub fn config_value(&self, key: &str) -> Option<String> {
        self.config(key)
    }

    /// Sets values in the repository's own `.git/config` (never the user's global one).
    /// Each entry: section, subsection, key, value. Reopen the repository to see them.
    pub(crate) fn set_local_config(&self, values: &[(&str, &str, &str, &str)]) -> Result<()> {
        set_local_config(self.repo.common_dir(), values)
    }

    /// `user.name` and `user.email` from the Git configuration, if both are set.
    pub fn config_identity(&self) -> Option<GitIdentity> {
        let name = self.config("user.name")?.trim().to_owned();
        let email = self.config("user.email")?.trim().to_owned();
        (!name.is_empty() && !email.is_empty()).then_some(GitIdentity { name, email })
    }

    /// The identity commits are made with: Git configuration first, then `fallback` (the app's
    /// own identity).
    pub(crate) fn identity_or(&self, fallback: Option<&GitIdentity>) -> Result<GitIdentity> {
        self.config_identity()
            .or_else(|| fallback.cloned())
            .ok_or_else(|| fail("no author identity: set your name and email first"))
    }
}

/// `user.name` and `user.email` from the user's global (and system) Git configuration, for
/// when no repository is open. `None` when isolated.
pub fn global_identity(options: &Options) -> Option<GitIdentity> {
    if options.isolated {
        return None;
    }
    let config = gix::config::File::from_globals().ok()?;
    let value = |key: &str| {
        config
            .string(key)
            .and_then(|v| v.to_str().ok().map(|s| s.trim().to_owned()))
            .filter(|s| !s.is_empty())
    };
    Some(GitIdentity {
        name: value("user.name")?,
        email: value("user.email")?,
    })
}

/// Writes `values` (section, subsection, key, value) into `<git_dir>/config`.
pub(crate) fn set_local_config(git_dir: &Path, values: &[(&str, &str, &str, &str)]) -> Result<()> {
    let path = git_dir.join("config");
    let mut file =
        gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)
            .ctx("cannot read the repository configuration")?;
    for (section, subsection, key, value) in values {
        let sub = (!subsection.is_empty()).then(|| gix::bstr::BStr::new(subsection.as_bytes()));
        file.set_raw_value_by(*section, sub, *key, gix::bstr::BStr::new(value.as_bytes()))
            .ctx("cannot change the repository configuration")?;
    }
    let mut out = Vec::new();
    file.write_to(&mut out)
        .ctx("cannot write the repository configuration")?;
    let tmp = git_dir.join("config.rt-tmp");
    std::fs::write(&tmp, &out).ctx("cannot write the repository configuration")?;
    std::fs::rename(&tmp, &path).ctx("cannot write the repository configuration")
}

/// Creates a repository in the collection folder `root` (branch `main` unless
/// `init.defaultBranch` says otherwise). Refuses if `root` already is a work tree.
pub fn init(root: &Path, options: &Options) -> Result<()> {
    let root = canonical(root).ctx("cannot open the collection folder")?;
    if root.join(".git").exists() {
        return Err(fail("the collection folder already is a Git repository"));
    }
    gix::ThreadSafeRepository::init_opts(
        &root,
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        options.open(),
    )
    .ctx("cannot create the repository")?;
    Ok(())
}

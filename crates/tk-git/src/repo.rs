// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Opening a repository (the whole of it, or the folder of a product's documents), what a
//! product plugs into it ([`Profile`]), and the small pieces every operation needs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use tk_git_types::{CommitScanner, FileMerger, GitIdentity, RefNamespace, ScopeFiles};

use crate::util::{Context, Result, Scope, canonical, fail};

/// A merge, cherry-pick or revert that stopped on conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Underway {
    Merge,
    CherryPick,
    Revert,
}

/// How the Git core names the product in what it writes and says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Product {
    /// Display name: the committer of reflog entries when none is configured (a fresh clone),
    /// of the core's own snapshot commits (undo, working-tree backups) and their messages.
    pub name: &'static str,
    /// Email of that committer.
    pub email: &'static str,
    /// The HTTP user agent, also the `agent=` capability of a push.
    pub agent: &'static str,
    /// What messages call the scope [`Repo::open`] takes (e.g. `folder`).
    pub scope: &'static str,
    /// What messages call its folder (e.g. `folder`).
    pub scope_folder: &'static str,
}

/// What a product plugs into every repository it opens: its name, its namespace (seam N8),
/// its file merger (N1), commit scanner (N2) and scope files (N3). [`Profile::new`] is the
/// generic core: Git's line merge for every file, nothing checked, every regular file counts.
#[derive(Clone)]
pub struct Profile {
    pub product: Product,
    pub namespace: RefNamespace,
    pub merger: Option<Arc<dyn FileMerger>>,
    pub scanner: Option<Arc<dyn CommitScanner>>,
    pub files: Option<Arc<dyn ScopeFiles>>,
}

impl Profile {
    /// The generic core for `product`, writing into `namespace`.
    pub fn new(product: Product, namespace: RefNamespace) -> Self {
        Self {
            product,
            namespace,
            merger: None,
            scanner: None,
            files: None,
        }
    }
}

impl std::fmt::Debug for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("product", &self.product)
            .field("namespace", &self.namespace)
            .field("merger", &self.merger.is_some())
            .field("scanner", &self.scanner.is_some())
            .field("files", &self.files.is_some())
            .finish()
    }
}

/// How repositories are opened. Every function that opens one takes `&impl AsOptions`, so a
/// product can pass its own options type that knows its [`Profile`].
#[derive(Debug, Clone)]
pub struct Options {
    /// Read only the repository's own configuration: no global or system Git configuration,
    /// no `GIT_*` environment variables. For tests; the app reads the user's configuration.
    pub isolated: bool,
    /// What every repository opened with these options starts with (`Repo::with_*` changes it
    /// per repository).
    pub profile: Profile,
}

impl Options {
    /// The user's configuration, `profile`'s parts.
    pub fn new(profile: Profile) -> Self {
        Self {
            isolated: false,
            profile,
        }
    }

    pub(crate) fn open(&self) -> gix::open::Options {
        let base = if self.isolated {
            gix::open::Options::isolated()
        } else {
            gix::open::Options::default()
        };
        // Reflog entries need a committer even when none is configured (a fresh clone).
        // Commits never use these: they get an explicit identity (`Repo::identity_or`).
        let product = self.profile.product;
        base.config_overrides([
            format!("gitoxide.committer.nameFallback={}", product.name),
            format!("gitoxide.committer.emailFallback={}", product.email),
        ])
    }

    pub(crate) fn product(&self) -> Product {
        self.profile.product
    }
}

/// What the functions that open a repository take: [`Options`] itself, or a product's own
/// options type that adds its [`Profile`].
pub trait AsOptions {
    fn options(&self) -> Options;
}

impl AsOptions for Options {
    fn options(&self) -> Options {
        self.clone()
    }
}

/// A repository: the whole of it, or one folder of it (a product's documents).
pub struct Repo {
    pub(crate) repo: gix::Repository,
    /// Canonical work tree root.
    pub(crate) workdir: PathBuf,
    /// Canonical root of the scope.
    pub(crate) root: PathBuf,
    pub(crate) scope: Scope,
    pub(crate) options: Options,
    /// What commits are signed and verified with (wave 15); the default signs nothing.
    pub(crate) signing: crate::sign::Signing,
    /// Where the app's own refs and files live (seam N8); the profile's by default.
    pub(crate) namespace: RefNamespace,
    /// What merges files on their model (seam N1); the profile's by default, `None`: Git's
    /// line merge for every file.
    pub(crate) merger: Option<Arc<dyn FileMerger>>,
    /// What checks the files of a commit (seam N2); the profile's by default, `None`: nothing
    /// is checked.
    pub(crate) scanner: Option<Arc<dyn CommitScanner>>,
    /// Which files of the scope count (seam N3); the profile's by default, `None`: every
    /// regular file.
    pub(crate) files: Option<Arc<dyn ScopeFiles>>,
}

/// The folder at or above `start` that holds a `.git` (folder, or file for linked work trees).
fn find_workdir(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

impl Repo {
    /// The repository containing the folder `root`, scoped to that folder (a product's
    /// documents: paths in and out are relative to it); `None` if there is none.
    pub fn open(root: &Path, options: &impl AsOptions) -> Result<Option<Self>> {
        let options: Options = options.options();
        let product = options.product();
        let root = canonical(root).ctx(&format!("cannot open the {}", product.scope_folder))?;
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
        let rel = root.strip_prefix(&workdir).map_err(|_| {
            fail(format!(
                "the {} is not inside the repository's work tree",
                product.scope
            ))
        })?;
        let mut prefix = String::new();
        for part in rel.components() {
            let part = part
                .as_os_str()
                .to_str()
                .ok_or_else(|| fail(format!("the {} path is not valid UTF-8", product.scope)))?;
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
        }
        let scope = Scope::folder(&prefix).map_err(|_| {
            fail(format!(
                "the {} cannot be a Git path: {prefix}",
                product.scope_folder
            ))
        })?;
        let profile = options.profile.clone();
        Ok(Some(Self {
            repo,
            workdir,
            root,
            scope,
            options,
            signing: crate::sign::Signing::default(),
            namespace: profile.namespace,
            merger: profile.merger,
            scanner: profile.scanner,
            files: profile.files,
        }))
    }

    /// The whole repository whose work tree contains `path` (any folder in it), scoped to the
    /// work tree root (seam N3: GitTailor's form); `None` if there is none.
    pub fn open_repository(path: &Path, options: &impl AsOptions) -> Result<Option<Self>> {
        let path = canonical(path).ctx("cannot open the folder")?;
        match find_workdir(&path) {
            Some(workdir) => Self::open(&workdir, options),
            None => Ok(None),
        }
    }

    /// Like [`Repo::open`], but "not a repository" is an error.
    pub fn require(root: &Path, options: &impl AsOptions) -> Result<Self> {
        let options: Options = options.options();
        let scope = options.product().scope;
        Self::open(root, &options)?
            .ok_or_else(|| fail(format!("the {scope} is not in a Git repository")))
    }

    /// The same repository opened again (after its configuration changed on disk), with the
    /// same plugged-in parts.
    pub(crate) fn reopen(&self) -> Result<Self> {
        Ok(Self::require(&self.root, &self.options)?
            .with_signing(self.signing.clone())
            .with_namespace(self.namespace)
            .with_merger(self.merger.clone())
            .with_scanner(self.scanner.clone())
            .with_files(self.files.clone()))
    }

    /// Signs and verifies commits with `signing` (a distribution's hook, `sign`); without it
    /// every commit stays unsigned.
    pub fn with_signing(mut self, signing: crate::sign::Signing) -> Self {
        self.signing = signing;
        self
    }

    /// Keeps backups, review refs and the action journal in `namespace` (seam N8) instead of
    /// the profile's.
    pub fn with_namespace(mut self, namespace: RefNamespace) -> Self {
        self.namespace = namespace;
        self
    }

    /// Merges files with `merger` (seam N1) instead of the profile's; `None` merges every file
    /// line by line, like Git.
    pub fn with_merger(mut self, merger: Option<Arc<dyn FileMerger>>) -> Self {
        self.merger = merger;
        self
    }

    /// Checks the files of every commit with `scanner` (seam N2) instead of the profile's;
    /// `None` checks nothing.
    pub fn with_scanner(mut self, scanner: Option<Arc<dyn CommitScanner>>) -> Self {
        self.scanner = scanner;
        self
    }

    /// Counts the files `files` includes (seam N3) instead of the profile's; `None` counts
    /// every regular file of the scope.
    pub fn with_files(mut self, files: Option<Arc<dyn ScopeFiles>>) -> Self {
        self.files = files;
        self
    }

    /// The part of the repository this `Repo` works on.
    pub fn scope(&self) -> &tk_git_types::RepoScope {
        self.scope.scope()
    }

    /// Whether the scope-relative `rel` is one of the files that count.
    pub(crate) fn includes(&self, rel: &str) -> bool {
        self.files.as_ref().is_none_or(|f| f.includes(rel))
    }

    /// Drops the files of nested units (as the scope files say) from a revision's files.
    pub(crate) fn drop_nested(&self, files: &mut std::collections::BTreeMap<String, String>) {
        if let Some(f) = &self.files {
            f.drop_nested(files);
        }
    }

    /// Refuses a commit of `files` (path, content as committed) when the scanner does
    /// (`GitError::SecretsFound`).
    pub(crate) fn scan_commit(&self, files: &[(&str, &[u8])]) -> Result<()> {
        match self.scanner.as_ref().and_then(|s| s.refusal(files)) {
            Some(message) => Err(crate::GitError::SecretsFound(message)),
            None => Ok(()),
        }
    }

    /// Where the app's own refs and files live.
    pub fn namespace(&self) -> RefNamespace {
        self.namespace
    }

    /// How the core names the product.
    pub fn product(&self) -> Product {
        self.options.product()
    }

    /// The reflog message `<namespace>: <what>` of a ref the core moves.
    pub(crate) fn reflog_message(&self, what: &str) -> String {
        format!("{}: {what}", self.namespace.name())
    }

    /// Canonical root of the scope.
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

    /// The commit `HEAD` points to (full hex id); `None` in a repository without commits.
    pub fn head_oid(&self) -> Result<Option<String>> {
        Ok(self.head_commit()?.map(|id| id.to_string()))
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

    /// A repository path from a path of the API: scope-relative, or repository-relative
    /// with a leading `/` (the wave-12 convention).
    pub(crate) fn repo_path(&self, path: &str) -> Result<BString> {
        match path.strip_prefix('/') {
            Some(rest) => Scope::whole().abs(rest),
            None => self.scope.abs(path),
        }
    }

    /// The API form of a repository path: scope-relative inside the scope,
    /// repository-relative with a leading `/` outside it.
    pub(crate) fn api_path(&self, path: &gix::bstr::BStr) -> String {
        self.scope
            .rel(path)
            .unwrap_or_else(|| format!("/{}", path.to_str_lossy()))
    }

    /// The same repository seen from its work tree root: no folder scope, every file.
    /// Git actions (wave 14) work on the whole repository.
    pub(crate) fn whole(&self) -> Result<Self> {
        let repo = gix::open_opts(&self.workdir, self.options.open())
            .ctx("cannot open the Git repository")?;
        Ok(Self {
            repo,
            workdir: self.workdir.clone(),
            root: self.workdir.clone(),
            scope: Scope::whole(),
            options: self.options.clone(),
            signing: self.signing.clone(),
            namespace: self.namespace,
            merger: self.merger.clone(),
            scanner: self.scanner.clone(),
            files: self.files.clone(),
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
pub fn global_identity(options: &impl AsOptions) -> Option<GitIdentity> {
    if options.options().isolated {
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

/// Creates a repository in the folder `root` (branch `main` unless `init.defaultBranch` says
/// otherwise). Refuses if `root` already is a work tree.
pub fn init(root: &Path, options: &impl AsOptions) -> Result<()> {
    let options: Options = options.options();
    let folder = options.product().scope_folder;
    let root = canonical(root).ctx(&format!("cannot open the {folder}"))?;
    if root.join(".git").exists() {
        return Err(fail(format!("the {folder} already is a Git repository")));
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

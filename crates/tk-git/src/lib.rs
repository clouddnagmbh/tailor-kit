// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `tk-git`: the Git core of the Tailor Kit, for any repository, on native targets, with
//! gitoxide (`gix`): no installed `git` needed. Answers with the types of `tk_git_types`, which
//! a shell that runs Git elsewhere (e.g. isomorphic-git in a browser) can answer with too.
//!
//! Origin: moved from RequestTailor's `rt-git` (wave 4 onwards); the product's parts (its file
//! merger, commit scanner, scope files, secret store and history rescue) stay there.
//!
//! # Products
//!
//! A product opens repositories with [`Options`] carrying its [`Profile`]: how the core names
//! it ([`Product`]: committer fallback, user agent, wording of the scope), its namespace for refs
//! and files of its own (seam N8, `tk_git_types::RefNamespace`), and optionally a file merger
//! (N1), a commit scanner (N2) and scope files (N3). [`Profile::new`] is the generic core: Git's
//! line merge for every file, no commit check, every regular file of the scope counts. Every
//! function that opens a repository takes `&impl AsOptions`, so a product can pass an options
//! type of its own that knows its profile. `Repo::with_*` changes one part per repository.
//!
//! # Model
//!
//! - [`Repo::open`] finds the repository that contains a folder and works on that folder (its
//!   scope); the repository may start above it. Paths in and out are relative to the scope root,
//!   `/`-separated; files outside it are never listed. [`Repo::open_repository`] takes the whole
//!   repository from any folder in it.
//! - No staging area: [`Repo::status`] compares the working tree with `HEAD`, and
//!   [`Repo::commit`] writes `HEAD`'s tree with exactly the given paths taken from the working
//!   tree. The index is kept in step for those paths, so `git status` on the command line
//!   agrees.
//! - Operations that change the working tree ([`Repo::checkout`], [`Repo::pull`]) refuse when
//!   a tracked file they would change has local changes, or an untracked file is in the way.
//!   Nothing is ever overwritten that is not committed. (Refusing while the app has unsaved
//!   changes is the shell's job; it knows about them.)
//! - Pull is fetch + fast-forward, or a merge commit. On conflicts the merge stops like
//!   `git merge`: `MERGE_HEAD` is written, the index holds the conflict stages, the files carry
//!   conflict markers; [`Repo::resolve`] picks a side per file and [`Repo::commit`] concludes
//!   the merge.
//!
//! # Remotes
//!
//! - `http://` and `https://`: fetch through gitoxide (reqwest, rustls on ring). Push is our
//!   own client for the smart HTTP protocol (`git-receive-pack`, see [`mod@http`]), because
//!   gitoxide cannot push yet.
//! - Local paths and `file://`: both directions run in-process, the other repository opened
//!   with gitoxide and the missing objects copied ([`mod@local`]). No `git` program is spawned.
//! - SSH and `git://` are not supported (`GitError::Unsupported`).
//! - A remote that asks for credentials, or rejects them, fails with `GitError::Auth(host)`.
//!   Credentials come from a [`CredentialSource`] (the product's secret store); they are sent
//!   over plain `http://` only to loopback hosts, elsewhere HTTPS is required. The host string
//!   (with `:port` when the URL names one) is what `GitError::Auth` carries.
//!
//! # Identity
//!
//! Commits use `user.name`/`user.email` from the Git configuration (repository, then global).
//! When those are missing, the app's own identity is used: [`identity::save`] keeps it as
//! JSON in the app's data folder. The user's Git configuration is never written, except the
//! repository's own `.git/config` for remote and upstream settings (`init`, `clone`, first
//! push of a branch, checkout of a remote branch).
//!
//! # Revisions (wave 12)
//!
//! [`Repo::rev_parse`], [`Repo::tree_at`], [`Repo::show`], [`Repo::merge_base`],
//! [`Repo::commits`] and [`Repo::file_log`] read any commit without touching the work tree.
//! [`Repo::walk_history`] and [`Repo::rewrite_blobs`] walk and rewrite the whole history (the
//! building blocks of a secret rescue).
//!
//! # Git actions (wave 14)
//!
//! [`Repo::plan`], [`Repo::apply`], [`Repo::undo`], [`Repo::last_action`], [`Repo::remotes`]
//! and [`Repo::stashes`] back the Git wizards (`tk_git_types::actions`). They work on the whole
//! repository (paths with a leading `/`), write backup refs and an action journal before
//! anything moves, and merge, cherry-pick and revert with the pull's merge (`merge.rs`).
//!
//! # Commits without a checkout (wave 13)
//!
//! [`Repo::commit_to_branch`] writes files onto a (new) branch: the branch's or a start
//! revision's tree with the files written or removed. Only that branch ref moves; the work
//! tree, the index and `HEAD` stay as they are, and the checked-out branch is refused.
//!
//! # Git client (wave 15)
//!
//! Any repository (`tk_git_types::{files, history, plumbing, index, signing, repos}`): the
//! working copy, file tree and blobs, blame, history search and reflog, the plumbing (`pick`,
//! `write_tree`, `write_commit`), the index operations and signing ([`sign`]).

/// Wave 15, strand D: amend, abort, restore and rewrite actions, text conflict result.
mod abort;
/// Wave 14, strand G: the Git wizards' actions (plan, apply, undo).
mod actions;
mod amend;
/// Wave 15, strand C: blame, history search and reflog.
mod blame;
mod branch_actions;
mod cherry_pick;
pub mod credentials;
pub mod http;
pub mod identity;
pub mod local;

mod blob;
mod branch;
mod changelog_commits;
mod commit;
/// Wave 12, strand D: `fileLog`.
mod file_log;
mod file_tree;
/// Wave 14, strand F: the commit graph (`graph`, `refs`, `changes`, `fileDiff`).
mod graph;
/// Wave 12, strand G: walking and rewriting the whole history.
mod history_rewrite;
mod history_search;
mod index_ops;
mod merge;
mod net;
mod pack;
mod patch;
mod plumbing;
mod reflog;
mod remote;
mod remotes_admin;
mod repo;
mod resolve_text;
mod restore;
mod revert;
/// Wave 12, strand E: fetching a PR head for review.
pub mod review;
/// Wave 12 interface: reading revisions without a checkout.
mod revisions;
mod rewrite_branch;
/// Wave 15 interface: commit signing and verification hooks for distributions (strand F).
pub mod sign;
mod signing;
mod stash;
mod status;
mod tags;
/// Committing files onto a branch without a checkout (since wave 13).
mod tree_commit;
mod undo;
mod util;

/// Errors of the Git core (seam N7); a product turns them into its own.
pub use tk_git_types::{GitError, GitResult};

mod working_copy;
mod worktree;

pub use actions::ActionOptions;
pub use changelog_commits::CommitMessage;
pub use credentials::{CredentialSource, Credentials, NoCredentials};
pub use history_rewrite::{
    BranchRewrite, HistoryEdit, HistoryRewrite, HistoryTips, HistoryVisitor,
};
pub use remote::{clone, clone_with_home, expand_home};
pub use repo::{AsOptions, Options, Product, Profile, Repo, global_identity, init};
pub use tree_commit::TreeFile;
pub use util::canonical;

/// A URL for the local repository at `path`: `file://` on Unix, the plain path on Windows
/// (where `file://C:\x` would read as a host). Both are handled in-process.
pub fn local_url(path: &std::path::Path) -> String {
    if cfg!(windows) {
        path.to_string_lossy().replace('\\', "/")
    } else {
        format!("file://{}", path.display())
    }
}

/// Status of the folder `root`: `repo: false` (everything else empty) when it is not in a Git
/// repository.
pub fn status(
    root: &std::path::Path,
    options: &impl AsOptions,
) -> Result<tk_git_types::GitStatus, crate::util::GitError> {
    match Repo::open(root, options)? {
        Some(repo) => repo.status(),
        None => Ok(tk_git_types::GitStatus {
            repo: false,
            branch: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            files: Vec::new(),
            merging: false,
        }),
    }
}

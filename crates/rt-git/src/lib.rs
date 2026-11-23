// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `rt-git`: Git for the repository that holds a collection, on native targets, with gitoxide
//! (`gix`): no installed `git` needed. Answers with the types of `rt_app::git`, which the web
//! app's isomorphic-git implementation answers with too.
//!
//! I/O happens here (repository on disk, network); `rt-app` stays I/O-free. The desktop app
//! calls this from `apps/desktop/src-tauri/src/git.rs`.
//!
//! # Model
//!
//! - [`Repo::open`] finds the repository that contains the collection folder; it may start
//!   above it. Paths in and out are relative to the collection root, `/`-separated; files
//!   outside the collection are never listed.
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
//! - SSH and `git://` are not supported (`AppError::Unsupported`).
//! - A remote that asks for credentials, or rejects them, fails with
//!   `AppError::GitAuth(host)`. Credentials are sent over plain `http://` only to loopback
//!   hosts; elsewhere HTTPS is required.
//!
//! # Credentials
//!
//! HTTPS credentials live in the app's [`SecretStore`](rt_app::SecretStore) (the OS keychain
//! in the desktop app), never in a file or in the Git configuration, and are never logged.
//! Key scheme ([`credentials::key`]):
//!
//! | `SecretKey` field | value |
//! | --- | --- |
//! | `collection` | `git:credentials` (collection ids are absolute paths or random ids, so this never collides) |
//! | `environment` | the host, with `:port` when the URL names one, e.g. `github.com`, `127.0.0.1:8080` |
//! | `variable` | `username` or `password` (two entries per host) |
//!
//! The same host string is what `AppError::GitAuth` carries, so the UI can pass it straight
//! back to `setCredentials`.
//!
//! # Identity
//!
//! Commits use `user.name`/`user.email` from the Git configuration (repository, then global).
//! When those are missing, the app's own identity is used: [`identity::save`] keeps it as
//! JSON in the app's data folder. The user's Git configuration is never written, except the
//! repository's own `.git/config` for remote and upstream settings (`init`, `clone`, first
//! push of a branch, checkout of a remote branch).
//!
//! Owner: wave 4, strand A1.
//!
//! # Revisions (wave 12)
//!
//! [`Repo::rev_parse`], [`Repo::tree_at`], [`Repo::show`], [`Repo::merge_base`],
//! [`Repo::commits`] and [`Repo::file_log`] read any commit without touching the work tree
//! (`GitApi` of wave 12). Wave 12 owners: `merge.rs` strand B (semantic pull and
//! `rt merge-driver`), `file_log.rs` strand D, `review.rs` strand E, `rewrite.rs` strand G;
//! `revisions.rs` is interface.
//!
//! # Git actions (wave 14)
//!
//! [`Repo::plan`], [`Repo::apply`], [`Repo::undo`], [`Repo::last_action`], [`Repo::remotes`]
//! and [`Repo::stashes`] back the Git wizards (`rt_app::git::actions`). They work on the whole
//! repository (paths with a leading `/`), write backup refs and an action journal before
//! anything moves, and merge, cherry-pick and revert with the pull's merge, generalised to
//! every collection of the repository (`merge.rs`). Owners: wave 14, strand G (`actions.rs`,
//! `branch_actions.rs`, `stash.rs`, `undo.rs`, `cherry_pick.rs`, `revert.rs`, `tags.rs`,
//! `remotes_admin.rs`).
//!
//! # Commits without a checkout (wave 13)
//!
//! [`Repo::commit_to_branch`] writes files onto a (new) branch: the branch's or a start
//! revision's tree with the files written or removed. Only that branch ref moves; the work
//! tree, the index and `HEAD` stay as they are, and the checked-out branch is refused.
//!
//! # Git client (wave 15)
//!
//! Any repository, not only one that holds a collection (`rt_app::git::{repos, files, history,
//! plumbing, index, signing}`). The interface adds the types and [`sign`]; the strands add
//! `Repo` methods in their own files: strand B the working copy, file tree and blobs, strand C
//! blame, history search and reflog, strand D the new actions and the plumbing (`pick`,
//! `write_tree`, `write_commit`), strand F the index operations and signing.

/// Wave 14, strand G: the Git wizards' actions (plan, apply, undo).
mod actions;
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
mod merge;
mod net;
mod pack;
mod remote;
mod remotes_admin;
mod repo;
mod revert;
/// Wave 12, strand E: fetching a PR head for review.
pub mod review;
/// Wave 12 interface: reading revisions without a checkout.
mod revisions;
/// Wave 12, strand G: history scan and rewrite.
pub mod rewrite;
/// Wave 15 interface: commit signing and verification hooks for distributions (strand F).
pub mod sign;
mod stash;
mod status;
mod tags;
/// Committing files onto a branch without a checkout (since wave 13).
mod tree_commit;
mod undo;
mod util;
mod working_copy;
mod worktree;

pub use actions::ActionOptions;
pub use credentials::{CredentialSource, Credentials, NoCredentials, StoreCredentials};
pub use remote::{clone, clone_with_home, expand_home};
pub use repo::{Options, Repo, global_identity, init};
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

/// Status of the collection at `root`: `repo: false` (everything else empty) when it is not
/// in a Git repository.
pub fn status(
    root: &std::path::Path,
    options: &Options,
) -> Result<rt_app::git::GitStatus, rt_app::AppError> {
    match Repo::open(root, options)? {
        Some(repo) => repo.status(),
        None => Ok(rt_app::git::GitStatus {
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

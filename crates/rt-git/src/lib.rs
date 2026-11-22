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

pub mod credentials;
pub mod http;
pub mod identity;
pub mod local;

mod branch;
mod commit;
mod merge;
mod net;
mod pack;
mod remote;
mod repo;
mod status;
mod util;
mod worktree;

pub use credentials::{CredentialSource, Credentials, NoCredentials, StoreCredentials};
pub use remote::clone;
pub use repo::{Options, Repo, global_identity, init};

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

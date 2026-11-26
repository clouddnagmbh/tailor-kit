// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `rt-git`: Git for the repository that holds a collection, on native targets. The Git core
//! is the Tailor Kit's `tk-git` (re-exported whole, so every `rt_git::…` path of before keeps
//! working); this crate plugs RequestTailor's parts into it:
//!
//! - [`Options`] (same fields as before) opens every repository with RequestTailor's
//!   [`profile`]: the product names of [`PRODUCT`] (committer fallback `RequestTailor`, user
//!   agent `git/rt-git`, messages about "the collection"), the namespace
//!   [`rt_app::git::REF_NAMESPACE`] (seam N8), the [`CollectionMerger`] (N1), the
//!   [`SecretScanner`] (N2) and the [`CollectionFiles`] (N3). `Repo::open(collection, &options)`
//!   therefore behaves exactly as it did before the seam; `Repo::with_*` still changes one part.
//! - [`RepoExt`]: what needs collection knowledge on top of the core: the secret rescue
//!   (`scan_history`, `rewrite_history`, `rt_app::rescue`), the merge resolution per field
//!   (`merge_state`, `resolve_merge`, `rt_app::merges`), `changelog_commits`
//!   (`rt_app::contract::ChangelogCommit`) and `collection_prefix`.
//! - [`credentials`]: HTTPS credentials in the app's [`SecretStore`](rt_app::SecretStore).
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

use std::sync::Arc;

pub use tk_git::*;

mod collection_files;
mod collection_merge;
pub mod credentials;
mod merge_state;
mod rescue;
mod secret_scanner;

pub use collection_files::CollectionFiles;
pub use collection_merge::CollectionMerger;
pub use credentials::StoreCredentials;
pub use secret_scanner::SecretScanner;

/// How the Git core names RequestTailor (the same strings as before the seam).
pub const PRODUCT: Product = Product {
    name: "RequestTailor",
    email: "requesttailor@localhost",
    agent: "git/rt-git",
    scope: "collection",
    scope_folder: "collection folder",
};

/// RequestTailor's parts in the Git core: its namespace, collection merger, secret scanner and
/// collection files.
pub fn profile() -> Profile {
    Profile {
        product: PRODUCT,
        namespace: rt_app::git::REF_NAMESPACE,
        merger: Some(Arc::new(CollectionMerger)),
        scanner: Some(Arc::new(SecretScanner)),
        files: Some(Arc::new(CollectionFiles)),
    }
}

/// How repositories are opened, with RequestTailor's [`profile`]. Every function of the core
/// that opens a repository takes it (`&impl tk_git::AsOptions`).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Read only the repository's own configuration: no global or system Git configuration,
    /// no `GIT_*` environment variables. For tests; the app reads the user's configuration.
    pub isolated: bool,
}

impl tk_git::AsOptions for Options {
    fn options(&self) -> tk_git::Options {
        tk_git::Options {
            isolated: self.isolated,
            profile: profile(),
        }
    }
}

/// What RequestTailor does with a repository on top of the core (collection knowledge).
pub trait RepoExt {
    /// Scans every commit of `HEAD` and the local branches with the secret scanner; one
    /// finding per value (see `rt_app::rescue`). `progress` hears about every few commits.
    fn scan_history(
        &self,
        progress: &mut dyn FnMut(rt_app::rescue::ScanProgress),
    ) -> GitResult<rt_app::rescue::HistoryScan>;

    /// Rewrites the local branches so the value of `plan`'s findings becomes
    /// `{{plan.variable}}` (every text file of the repository that holds it, inside the
    /// collection or not). Call it only after the user confirmed. `store` gets the value
    /// before any ref changes; `now` (Unix seconds) names the backups.
    fn rewrite_history(
        &self,
        plan: &rt_app::rescue::RescuePlan,
        now: i64,
        store: &mut dyn FnMut(&str) -> GitResult<()>,
    ) -> GitResult<rt_app::rescue::RescueResult>;

    /// The conflicts of the merge in progress per field, recomputed from the index stages
    /// (`MergeApi.conflicts`).
    fn merge_state(&self) -> GitResult<rt_app::merges::MergeState>;

    /// Resolves the merge in progress with `choices` (`MergeApi.resolve`): every conflicted
    /// file whose conflicts the choices all decide (or that merges cleanly on the model) is
    /// written and marked resolved; the others stay. Returns the paths written. The next
    /// `Repo::commit` concludes the merge.
    fn resolve_merge(&self, choices: &[rt_app::merges::ConflictChoice]) -> GitResult<Vec<String>>;

    /// Commits reachable from `to` along first parents but not from `from`, newest first, as
    /// `rt changelog` hands them to writers (`ChangelogFacts::commits`).
    fn changelog_commits(
        &self,
        from: &str,
        to: &str,
    ) -> GitResult<Vec<rt_app::contract::ChangelogCommit>>;

    /// Where the collection sits in the repository: `""` at the root, else `a/b` (no trailing
    /// slash). The key under which recorded results tell two collections of one repository
    /// apart (`rt_app::timeline::TestRecord::collection`).
    fn collection_prefix(&self) -> String;
}

impl RepoExt for Repo {
    fn scan_history(
        &self,
        progress: &mut dyn FnMut(rt_app::rescue::ScanProgress),
    ) -> GitResult<rt_app::rescue::HistoryScan> {
        rescue::scan_history(self, progress)
    }

    fn rewrite_history(
        &self,
        plan: &rt_app::rescue::RescuePlan,
        now: i64,
        store: &mut dyn FnMut(&str) -> GitResult<()>,
    ) -> GitResult<rt_app::rescue::RescueResult> {
        rescue::rewrite_history(self, plan, now, store)
    }

    fn merge_state(&self) -> GitResult<rt_app::merges::MergeState> {
        merge_state::merge_state(self)
    }

    fn resolve_merge(&self, choices: &[rt_app::merges::ConflictChoice]) -> GitResult<Vec<String>> {
        merge_state::resolve_merge(self, choices)
    }

    fn changelog_commits(
        &self,
        from: &str,
        to: &str,
    ) -> GitResult<Vec<rt_app::contract::ChangelogCommit>> {
        Ok(self
            .changelog_messages(from, to)?
            .into_iter()
            .map(|m| rt_app::contract::ChangelogCommit::from_message(&m.oid, &m.message, &m.author))
            .collect())
    }

    fn collection_prefix(&self) -> String {
        self.scope_prefix()
    }
}

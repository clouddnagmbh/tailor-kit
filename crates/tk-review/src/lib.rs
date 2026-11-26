// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Pull-request review: pull requests of GitHub and merge requests of GitLab behind one
//! interface, [`ReviewProvider`]. Gitea and Bitbucket can follow on the same interface.
//! Provenance: extracted from RequestTailor's `rt-app` `review` module (seam K2).
//!
//! - Local: the program talks only to the user's own Git host, with the user's own token.
//! - Calls go through [`Transport`](tk_net::transport::Transport): natively the HTTP stack, in
//!   the browser `fetch` or a companion (device-code login needs the companion: the hosts'
//!   login endpoints allow no CORS).
//! - Tokens: a personal access token, or one from a device-code login, kept like secret values
//!   ([`token_slot`]); never in a file, never logged, masked in errors.
//! - A PR is reviewed locally: its head is fetched into [`review_ref`] (no checkout).
//! - Comments may carry a product-defined anchor (`ReviewComment<A>`); the providers never
//!   set one.

use serde::{Deserialize, Serialize};
use tk_git_types::RefNamespace;
use tk_net::services::LocalBoxFuture;
use tk_net::transport::Transport;
use tk_secrets::SecretSlot;

mod client;
mod detect;
pub mod device;
mod github;
mod gitlab;
mod time;

pub use github::GithubProvider;
pub use gitlab::GitlabProvider;

/// [`SecretSlot::space`] of review tokens (app-wide). The strings are what existing keychains
/// hold: they must never change.
pub const TOKEN_COLLECTION: &str = "review:tokens";
/// [`SecretSlot::name`] of review tokens.
pub const TOKEN_VARIABLE: &str = "token";

/// Where the token for `host` (e.g. `github.com`, `gitlab.example.com:8443`) is kept.
pub fn token_slot(host: &str) -> SecretSlot {
    SecretSlot {
        space: TOKEN_COLLECTION.to_owned(),
        group: host.to_owned(),
        name: TOKEN_VARIABLE.to_owned(),
    }
}

/// The local ref a PR's head is fetched into, e.g. `refs/<namespace>/review/github/7`.
pub fn review_ref(namespace: &RefNamespace, kind: ReviewKind, number: u64) -> String {
    namespace.review_ref(kind.id(), number)
}

/// The local ref the PR's target branch is fetched into next to the head, so the merge base
/// exists locally (`refs/<namespace>/review/github/7-base`; not `<n>/base`, which Git cannot
/// keep next to the ref `<n>`).
pub fn review_base_ref(namespace: &RefNamespace, kind: ReviewKind, number: u64) -> String {
    namespace.review_base_ref(kind.id(), number)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewKind {
    Github,
    Gitlab,
}

impl ReviewKind {
    pub fn id(self) -> &'static str {
        match self {
            ReviewKind::Github => "github",
            ReviewKind::Gitlab => "gitlab",
        }
    }
}

/// A self-hosted Git host the user added (GitHub Enterprise, GitLab): `github.com` and
/// `gitlab.com` are known without one. Kept per device by the product, never in a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewHost {
    /// Host as in the remote URL, with `:port` if it has one.
    pub host: String,
    pub kind: ReviewKind,
    /// API base, e.g. `https://git.example.com/api/v3` or `https://gitlab.example.com/api/v4`.
    pub api: String,
}

/// The repository on its Git host, derived from the checkout's `origin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRepo {
    pub kind: ReviewKind,
    /// Host (with `:port` if the URL has one): the key of the token.
    pub host: String,
    /// API base, e.g. `https://api.github.com`, `https://gitlab.example.com/api/v4`.
    pub api: String,
    /// `owner/name` (GitHub) or the project path (GitLab, may have subgroups).
    pub project: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PullState {
    Open,
    Closed,
    Merged,
}

/// A pull request (GitHub) or merge request (GitLab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    /// PR number (GitHub) or MR IID (GitLab).
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: PullState,
    pub draft: bool,
    /// Branch names.
    pub source: String,
    pub target: String,
    /// Commit ids of the head and of the target the PR is based on.
    pub head: String,
    pub base: String,
    /// Web page of the PR.
    pub url: String,
    /// Unix seconds.
    pub updated: i64,
}

/// A comment on a PR: general, or on a line of a file. `A` is the product's own anchor for a
/// line (the providers never set one; the wire name stays `field`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment<A = ()> {
    pub id: String,
    pub author: String,
    /// Markdown.
    pub body: String,
    /// Unix seconds.
    pub created: i64,
    /// Path relative to the product's root folder, for a line comment (the shell converts from
    /// and to the repository-relative path the host uses; comments outside it keep theirs).
    pub path: Option<String>,
    /// 1-based line on the head side.
    pub line: Option<u32>,
    /// What the product maps that line to (e.g. a field of a structured document).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<A>,
    /// The line no longer exists at the head.
    pub outdated: bool,
}

/// A new line comment, at the PR's head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewReviewComment {
    /// Path relative to the product's root folder (the shell prefixes the folder).
    pub path: String,
    /// 1-based line on the head side.
    pub line: u32,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewVerdict {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSubmission {
    pub verdict: ReviewVerdict,
    pub body: String,
}

/// Wave 15: a pull request (merge request) to open from a branch that is on the remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPullRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// The branch with the changes, as named on the remote (`feature/login`).
    pub head: String,
    /// The branch to merge into (`main`).
    pub base: String,
    /// Open as a draft where the host has drafts.
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiState {
    Pending,
    Success,
    Failure,
    /// Neutral, skipped or cancelled.
    Neutral,
    /// No CI reported anything.
    None,
}

/// One check run (GitHub) or pipeline job (GitLab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiCheck {
    pub name: String,
    pub state: CiState,
    pub url: Option<String>,
}

/// CI status of the PR's head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiStatus {
    /// Combined: failure if any failed, pending if any is pending, else success.
    pub state: CiState,
    pub checks: Vec<CiCheck>,
}

/// An opened PR, fetched for review (`ReviewApi.open`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSession {
    pub pull: PullRequest,
    /// The local ref holding the head ([`review_ref`]).
    pub head_ref: String,
    /// Merge base of head and target, locally: the left side of the semantic diff.
    pub merge_base: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "kebab-case")]
pub enum ReviewError {
    /// No token for the host; the value is the host.
    #[error("no token for {0}")]
    NoToken(String),
    /// The host rejected the token (expired, revoked, missing scope).
    #[error("{0}")]
    Auth(String),
    #[error("not found: {0}")]
    NotFound(String),
    /// HTTP or transport failure (token masked).
    #[error("the Git host could not be reached: {0}")]
    Transport(String),
    /// The host answered with an error (status and message, token masked).
    #[error("the Git host refused: {0}")]
    Host(String),
}

/// A Git host's review API, for one repository and token. Not `Send` (wasm32).
pub trait ReviewProvider<A = ()> {
    fn repo(&self) -> &ReviewRepo;
    /// Open pull requests, most recently updated first.
    fn list(&self) -> LocalBoxFuture<'_, Result<Vec<PullRequest>, ReviewError>>;
    fn get(&self, number: u64) -> LocalBoxFuture<'_, Result<PullRequest, ReviewError>>;
    /// The ref on the remote that holds the PR's head, e.g. `refs/pull/7/head` (GitHub),
    /// `refs/merge-requests/7/head` (GitLab).
    fn head_ref(&self, number: u64) -> String;
    fn comments(
        &self,
        number: u64,
    ) -> LocalBoxFuture<'_, Result<Vec<ReviewComment<A>>, ReviewError>>;
    fn comment<'a>(
        &'a self,
        pull: &'a PullRequest,
        comment: &'a NewReviewComment,
    ) -> LocalBoxFuture<'a, Result<ReviewComment<A>, ReviewError>>;
    fn submit<'a>(
        &'a self,
        pull: &'a PullRequest,
        review: &'a ReviewSubmission,
    ) -> LocalBoxFuture<'a, Result<(), ReviewError>>;
    fn ci<'a>(&'a self, pull: &'a PullRequest)
    -> LocalBoxFuture<'a, Result<CiStatus, ReviewError>>;
    /// Wave 15: opens a pull request from a branch that is already on the remote. GitHub and
    /// GitLab implement it; a host that does not answers with an explanation.
    fn create<'a>(
        &'a self,
        pull: &'a NewPullRequest,
    ) -> LocalBoxFuture<'a, Result<PullRequest, ReviewError>> {
        let _ = pull;
        Box::pin(async {
            Err(ReviewError::Host(
                "this Git host does not support opening pull requests from the app".into(),
            ))
        })
    }
}

/// The review repository for a remote URL (`https://github.com/acme/orders.git`,
/// `git@gitlab.com:group/sub/api.git`, or one on a host in `hosts`). `None` for a remote the
/// app does not know (a local path, another host).
pub fn detect(remote_url: &str, hosts: &[ReviewHost]) -> Option<ReviewRepo> {
    detect::detect(remote_url, hosts)
}

/// The provider for `repo` with the user's `token`, over `transport`.
///
/// `user_agent` is the product name sent as `User-Agent` where the platform allows one (not in
/// the browser, whose `fetch` refuses it).
pub fn provider<'t, T: Transport, A: 't>(
    transport: &'t T,
    repo: &ReviewRepo,
    token: &str,
    user_agent: &str,
) -> Box<dyn ReviewProvider<A> + 't> {
    match repo.kind {
        ReviewKind::Github => Box::new(GithubProvider::new(
            transport,
            repo.clone(),
            token,
            user_agent,
        )),
        ReviewKind::Gitlab => Box::new(GitlabProvider::new(
            transport,
            repo.clone(),
            token,
            user_agent,
        )),
    }
}

/// Combined state of checks: failure if any failed, else pending if any is pending, else
/// success if any succeeded, else neutral (none at all: [`CiState::None`]).
pub fn combine(checks: &[CiCheck]) -> CiState {
    let any = |s: CiState| checks.iter().any(|c| c.state == s);
    if checks.is_empty() {
        CiState::None
    } else if any(CiState::Failure) {
        CiState::Failure
    } else if any(CiState::Pending) {
        CiState::Pending
    } else if any(CiState::Success) {
        CiState::Success
    } else {
        CiState::Neutral
    }
}

/// Checks a list of self-hosted hosts before it is saved: a host name, an `http(s)://` API
/// base, no duplicates. Returns it normalised (trimmed, no trailing `/` on the API base).
pub fn normalize_hosts(hosts: &[ReviewHost]) -> Result<Vec<ReviewHost>, ReviewError> {
    let mut out: Vec<ReviewHost> = Vec::new();
    for h in hosts {
        let host = h.host.trim().to_ascii_lowercase();
        let api = h.api.trim().trim_end_matches('/').to_owned();
        if host.is_empty() || host.contains('/') || host.contains(char::is_whitespace) {
            return Err(ReviewError::Host(format!(
                "{:?} is not a host name (e.g. git.example.com or git.example.com:8443)",
                h.host
            )));
        }
        if !(api.starts_with("https://") || api.starts_with("http://")) || api.len() < 9 {
            return Err(ReviewError::Host(format!(
                "the API address of {host} must start with https:// (or http://)"
            )));
        }
        if out.iter().any(|o| o.host == host) {
            return Err(ReviewError::Host(format!("{host} is listed twice")));
        }
        out.push(ReviewHost {
            host,
            kind: h.kind,
            api,
        });
    }
    Ok(out)
}

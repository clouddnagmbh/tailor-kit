// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Pull-request review in the app (wave 12): pull requests of GitHub and merge requests of
//! GitLab behind one interface, [`ReviewProvider`]. Gitea and Bitbucket can follow on the same
//! interface.
//!
//! - Free and local: the app talks only to the user's own Git host, with the user's own token;
//!   never to CloudDNA.
//! - Calls go through the app's [`Transport`](rt_core::transport::Transport), like the AI
//!   assistant: natively the app's HTTP stack, in the web app `fetch` or the companion
//!   (device-code login needs the companion: the hosts' login endpoints allow no CORS).
//! - Tokens: a personal access token, or one from a device-code login, kept like secret values
//!   ([`token_slot`]: OS keychain; web: encrypted browser storage); never in a file, never
//!   logged, masked in errors.
//! - A PR is reviewed locally: its head is fetched into [`review_ref`] (no checkout), the
//!   semantic diff is `DiffApi.between(base, head)`, the changed requests run with
//!   `CompareApi.run` at the head against the reviewer's environment.
//!
//! Owner: wave 12 interface; implementation strand E.

use rt_core::services::LocalBoxFuture;
use serde::{Deserialize, Serialize};

use crate::SecretKey;
use rt_format::diff::FieldPath;

/// [`SecretKey::collection`] of review tokens (app-wide, not a collection).
pub const TOKEN_COLLECTION: &str = "review:tokens";
/// [`SecretKey::variable`] of review tokens.
pub const TOKEN_VARIABLE: &str = "token";

/// Where the token for `host` (e.g. `github.com`, `gitlab.example.com:8443`) is kept.
pub fn token_slot(host: &str) -> SecretKey {
    SecretKey {
        collection: TOKEN_COLLECTION.to_owned(),
        environment: host.to_owned(),
        variable: TOKEN_VARIABLE.to_owned(),
    }
}

/// The local ref a PR's head is fetched into, e.g. `refs/requesttailor/review/github/7`.
pub fn review_ref(kind: ReviewKind, number: u64) -> String {
    format!("refs/requesttailor/review/{}/{number}", kind.id())
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
/// `gitlab.com` are known without one. Kept per device (natively `review/hosts.json` in the
/// data folder, in the web app in browser storage), never in the collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewHost {
    /// Host as in the remote URL, with `:port` if it has one.
    pub host: String,
    pub kind: ReviewKind,
    /// API base, e.g. `https://git.example.com/api/v3` or `https://gitlab.example.com/api/v4`.
    pub api: String,
}

/// The repository on its Git host, derived from the collection's `origin`.
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

/// A comment on a PR: general, or on a line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: String,
    pub author: String,
    /// Markdown.
    pub body: String,
    /// Unix seconds.
    pub created: i64,
    /// Repository-relative path, for a line comment.
    pub path: Option<String>,
    /// 1-based line on the head side.
    pub line: Option<u32>,
    /// The request field at that line, as the app maps it (semantic diff, `FieldChange.line`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<FieldPath>,
    /// The line no longer exists at the head.
    pub outdated: bool,
}

/// A new line comment, at the PR's head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewReviewComment {
    /// Repository-relative path.
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
pub trait ReviewProvider {
    fn repo(&self) -> &ReviewRepo;
    /// Open pull requests, most recently updated first.
    fn list(&self) -> LocalBoxFuture<'_, Result<Vec<PullRequest>, ReviewError>>;
    fn get(&self, number: u64) -> LocalBoxFuture<'_, Result<PullRequest, ReviewError>>;
    /// The ref on the remote that holds the PR's head, e.g. `refs/pull/7/head` (GitHub),
    /// `refs/merge-requests/7/head` (GitLab).
    fn head_ref(&self, number: u64) -> String;
    fn comments(&self, number: u64) -> LocalBoxFuture<'_, Result<Vec<ReviewComment>, ReviewError>>;
    fn comment<'a>(
        &'a self,
        pull: &'a PullRequest,
        comment: &'a NewReviewComment,
    ) -> LocalBoxFuture<'a, Result<ReviewComment, ReviewError>>;
    fn submit<'a>(
        &'a self,
        pull: &'a PullRequest,
        review: &'a ReviewSubmission,
    ) -> LocalBoxFuture<'a, Result<(), ReviewError>>;
    fn ci<'a>(&'a self, pull: &'a PullRequest)
    -> LocalBoxFuture<'a, Result<CiStatus, ReviewError>>;
}

/// The review repository for a remote URL (`https://github.com/acme/orders.git`,
/// `git@gitlab.com:group/sub/api.git`, or one on a host in `hosts`).
///
/// Interface state (wave 12): always `None`; strand E implements it.
pub fn detect(remote_url: &str, hosts: &[ReviewHost]) -> Option<ReviewRepo> {
    let _ = (remote_url, hosts);
    None
}

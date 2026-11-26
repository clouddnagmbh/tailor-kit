// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Pull-request review in the app (wave 12): pull requests of GitHub and merge requests of
//! GitLab behind one interface. The providers, host detection, device-code login and the
//! PR/comment types live in `tk-review` (seam K2); this module binds them to the app:
//!
//! - [`ReviewComment`] carries the request field a line maps to ([`FieldPath`]), the kit's
//!   comment with the app's anchor.
//! - [`token_slot`] returns the app's [`SecretKey`] (same three strings as the kit's slot, so
//!   stored tokens keep working); [`review_ref`] and [`review_base_ref`] use the app's ref
//!   namespace ([`crate::git::REF_NAMESPACE`]).
//! - [`provider`] sends `RequestTailor` as the `User-Agent`.
//! - Free and local: the app talks only to the user's own Git host, with the user's own token;
//!   never to CloudDNA.
//! - A PR is reviewed locally: its head is fetched into [`review_ref`] (no checkout), the
//!   semantic diff is `DiffApi.between(base, head)`, the changed requests run with
//!   `CompareApi.run` at the head against the reviewer's environment.
//!
//! Owner: wave 12 interface; implementation strand E.

use rt_core::transport::Transport;
use rt_format::diff::FieldPath;
pub use tk_review::{
    CiCheck, CiState, CiStatus, GithubProvider, GitlabProvider, NewPullRequest, NewReviewComment,
    PullRequest, PullState, ReviewError, ReviewHost, ReviewKind, ReviewRepo, ReviewSession,
    ReviewSubmission, ReviewVerdict, TOKEN_SPACE, TOKEN_VARIABLE, combine, normalize_hosts,
};

use crate::SecretKey;

pub mod device;

/// `User-Agent` of the calls to the Git hosts (natively; the browser sends its own).
const USER_AGENT: &str = "RequestTailor";

/// A comment on a PR: general, or on a line of a file; `field` is the request field at that
/// line, as the app maps it (semantic diff, `FieldChange.line`).
pub type ReviewComment = tk_review::ReviewComment<FieldPath>;

/// A Git host's review API, for one repository and token (the kit's [`tk_review::ReviewProvider`]
/// with the app's comment anchor). Not `Send` (wasm32).
pub trait ReviewProvider: tk_review::ReviewProvider<FieldPath> {}
impl<P: tk_review::ReviewProvider<FieldPath> + ?Sized> ReviewProvider for P {}

/// Where the token for `host` (e.g. `github.com`, `gitlab.example.com:8443`) is kept.
pub fn token_slot(host: &str) -> SecretKey {
    let slot = tk_review::token_slot(host);
    SecretKey {
        collection: slot.space,
        environment: slot.group,
        variable: slot.name,
    }
}

/// The local ref a PR's head is fetched into, e.g. `refs/requesttailor/review/github/7`.
pub fn review_ref(kind: ReviewKind, number: u64) -> String {
    tk_review::review_ref(&crate::git::REF_NAMESPACE, kind, number)
}

/// The local ref the PR's target branch is fetched into next to the head, so the merge base
/// exists locally (`refs/requesttailor/review/github/7-base`; not `<n>/base`, which Git
/// cannot keep next to the ref `<n>`).
pub fn review_base_ref(kind: ReviewKind, number: u64) -> String {
    tk_review::review_base_ref(&crate::git::REF_NAMESPACE, kind, number)
}

/// The review repository for a remote URL (`https://github.com/acme/orders.git`,
/// `git@gitlab.com:group/sub/api.git`, or one on a host in `hosts`). `None` for a remote the
/// app does not know (a local path, another host).
pub fn detect(remote_url: &str, hosts: &[ReviewHost]) -> Option<ReviewRepo> {
    tk_review::detect(remote_url, hosts)
}

/// The provider for `repo` with the user's `token`, over `transport`.
pub fn provider<'t, T: Transport>(
    transport: &'t T,
    repo: &ReviewRepo,
    token: &str,
) -> Box<dyn ReviewProvider + 't> {
    match repo.kind {
        ReviewKind::Github => Box::new(GithubProvider::new(
            transport,
            repo.clone(),
            token,
            USER_AGENT,
        )),
        ReviewKind::Gitlab => Box::new(GitlabProvider::new(
            transport,
            repo.clone(),
            token,
            USER_AGENT,
        )),
    }
}

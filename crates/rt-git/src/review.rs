// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Fetching a pull request's head (`refs/pull/<n>/head`, `refs/merge-requests/<n>/head`) into
//! `rt_app::review::review_ref` for a local review, without a checkout.
//!
//! The target branch comes along into `rt_app::review::review_base_ref`, so the merge base of
//! head and target exists locally even when `origin/<target>` is stale. Both refs are
//! overwritten on every open: they are ours, never the user's branches.

use std::cell::Cell;
use std::sync::atomic::AtomicBool;

use gix::ObjectId;
use gix::refs::transaction::PreviousValue;
use rt_app::AppError;
use rt_app::review::{ReviewKind, review_base_ref, review_ref};

use crate::credentials::{CredentialSource, Credentials};
use crate::net::{Auth, Endpoint, endpoint, ensure_tls};
use crate::repo::Repo;
use crate::util::{Context, Result, fail};

/// The user's token as the HTTPS password for the review host (and only that host): GitHub
/// takes any user name next to a token, GitLab wants `oauth2`.
pub struct TokenCredentials<'a> {
    pub host: &'a str,
    pub kind: ReviewKind,
    pub token: &'a str,
}

impl CredentialSource for TokenCredentials<'_> {
    fn get(&self, host: &str) -> std::result::Result<Option<Credentials>, AppError> {
        Ok(host.eq_ignore_ascii_case(self.host).then(|| Credentials {
            username: match self.kind {
                ReviewKind::Github => "x-access-token".to_owned(),
                ReviewKind::Gitlab => "oauth2".to_owned(),
            },
            password: self.token.to_owned(),
        }))
    }
}

/// What fetching a pull request left in the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedReview {
    /// The local ref holding the head ([`review_ref`]).
    pub head_ref: String,
    /// The head commit.
    pub head: String,
    /// The best common ancestor of head and target: the left side of the semantic diff.
    pub merge_base: String,
}

impl Repo {
    /// The URL of the remote `origin`, if there is one.
    pub fn origin_url(&self) -> Option<String> {
        self.config("remote.origin.url")
    }

    /// Fetches the head of pull request `number` (`remote_head_ref` on `origin`) and the tip of
    /// its `target` branch into the review refs. `creds` may supply the user's token for a
    /// private repository (user name and token as an HTTPS password).
    pub fn fetch_review(
        &self,
        kind: ReviewKind,
        number: u64,
        remote_head_ref: &str,
        target: &str,
        creds: &dyn CredentialSource,
    ) -> Result<FetchedReview> {
        let head_ref = review_ref(kind, number);
        let base_ref = review_base_ref(kind, number);
        let target_ref = format!("refs/heads/{}", target.trim_start_matches("refs/heads/"));
        let specs = [
            (remote_head_ref, head_ref.as_str()),
            (target_ref.as_str(), base_ref.as_str()),
        ];
        let url = self
            .config("remote.origin.url")
            .ok_or_else(|| fail("no remote named origin: add one first"))?;
        match endpoint(&url, &self.workdir)? {
            Endpoint::Local(path) => self.fetch_local(&path, &specs)?,
            Endpoint::Http(target) => self.fetch_http(&target, &specs, creds)?,
        }
        let head = self.rev_parse(&head_ref)?;
        let merge_base = self
            .merge_base(&head_ref, &base_ref)?
            .ok_or_else(|| fail("the pull request shares no history with its target branch"))?;
        Ok(FetchedReview {
            head_ref,
            head,
            merge_base,
        })
    }

    fn fetch_local(&self, path: &std::path::Path, specs: &[(&str, &str)]) -> Result<()> {
        let src = crate::local::open(path, &self.options)?;
        for (remote, local) in specs {
            let mut reference = src
                .find_reference(*remote)
                .map_err(|_| fail(format!("the remote has no {remote}")))?;
            let id: ObjectId = reference
                .peel_to_id()
                .ctx("cannot read a remote ref")?
                .detach();
            let ids = crate::local::missing_objects(&src, &[id], &|o| self.repo.has_object(o))?;
            crate::local::copy_objects(&src, &self.repo, &ids)?;
            self.repo
                .reference(*local, id, PreviousValue::Any, "review: fetch")
                .ctx("cannot update a review ref")?;
        }
        Ok(())
    }

    fn fetch_http(
        &self,
        target: &crate::net::HttpTarget,
        specs: &[(&str, &str)],
        creds: &dyn CredentialSource,
    ) -> Result<()> {
        ensure_tls();
        let auth = target.auth(creds)?;
        let asked = Cell::new(false);
        let refspecs: Vec<String> = specs.iter().map(|(r, l)| format!("+{r}:{l}")).collect();
        let result = (|| -> std::result::Result<(), gix::Error> {
            let remote = self
                .repo
                .find_remote("origin")
                .map_err(gix::Error::from_error)?
                .with_refspecs(
                    refspecs.iter().map(String::as_str),
                    gix::remote::Direction::Fetch,
                )
                .map_err(gix::Error::from_error)?;
            let connection = remote
                .connect(gix::remote::Direction::Fetch)
                .map_err(gix::Error::from_error)?
                .with_credentials(|action| match action {
                    gix::credentials::helper::Action::Get(ctx) => {
                        asked.set(true);
                        match &auth {
                            Auth::Some(c) => Ok(Some(gix::credentials::protocol::Outcome {
                                identity: gix::sec::identity::Account {
                                    username: c.username.clone(),
                                    password: c.password.clone(),
                                    oauth_refresh_token: None,
                                },
                                next: ctx.into(),
                            })),
                            _ => Err(gix::Error::from_error(gix::error::message(
                                "authentication required",
                            ))),
                        }
                    }
                    _ => Ok(None),
                });
            // Only the refspecs added above: the user's other branches stay as they are.
            let mut options = gix::remote::ref_map::Options::default();
            options.prefix_from_spec_as_filter_on_remote = true;
            connection
                .prepare_fetch(gix::progress::Discard, options)
                .map_err(gix::Error::from_error)?
                .receive(gix::progress::Discard, &AtomicBool::new(false))
                .map_err(gix::Error::from_error)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(()),
            Err(_) if asked.get() => Err(target.auth_error(&auth)),
            Err(e) => Err(AppError::Git(format!(
                "cannot fetch the pull request from {}: {e}",
                target.host
            ))),
        }
    }
}

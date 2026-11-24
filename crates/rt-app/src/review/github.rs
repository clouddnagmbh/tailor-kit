// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! GitHub (and GitHub Enterprise): pull requests, review comments, reviews, check runs.

use rt_core::services::LocalBoxFuture;
use rt_core::transport::Transport;
use serde_json::{Value, json};

use super::client::{Client, Reply, items, text};
use super::time::unix_seconds;
use super::{
    CiCheck, CiState, CiStatus, NewPullRequest, NewReviewComment, PullRequest, PullState,
    ReviewComment, ReviewError, ReviewProvider, ReviewRepo, ReviewSubmission, ReviewVerdict,
    combine,
};

/// Pages followed at most (100 per page).
const MAX_PAGES: usize = 5;

pub struct GithubProvider<'t, T: Transport> {
    client: Client<'t, T>,
    repo: ReviewRepo,
}

impl<'t, T: Transport> GithubProvider<'t, T> {
    pub fn new(transport: &'t T, repo: ReviewRepo, token: &str) -> Self {
        let client = Client::new(transport, token, &repo.api)
            .with_header("x-github-api-version", "2022-11-28");
        Self { client, repo }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/repos/{}{path}", self.client.api, self.repo.project)
    }

    /// Every item of a list endpoint, following `Link: rel="next"` on the API's own origin.
    async fn pages(&self, first: String) -> Result<Vec<Value>, ReviewError> {
        let mut out = Vec::new();
        let mut url = first;
        for _ in 0..MAX_PAGES {
            let reply = self.client.get(&url).await?;
            let next = next_link(&reply).filter(|n| self.client.same_origin(n));
            out.extend(items(reply.body));
            match next {
                Some(n) => url = n,
                None => break,
            }
        }
        Ok(out)
    }
}

/// The `rel="next"` URL of a `Link` header.
fn next_link(reply: &Reply) -> Option<String> {
    reply.header("link")?.split(',').find_map(|part| {
        let (url, rel) = part.split_once(';')?;
        rel.contains("rel=\"next\"").then(|| {
            url.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        })
    })
}

fn pull(v: &Value) -> PullRequest {
    let merged = v.get("merged_at").is_some_and(|m| !m.is_null());
    let state = match v.get("state").and_then(Value::as_str) {
        _ if merged => PullState::Merged,
        Some("closed") => PullState::Closed,
        _ => PullState::Open,
    };
    PullRequest {
        number: v.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: text(v, "/title"),
        author: text(v, "/user/login"),
        state,
        draft: v.get("draft").and_then(Value::as_bool).unwrap_or(false),
        source: text(v, "/head/ref"),
        target: text(v, "/base/ref"),
        head: text(v, "/head/sha"),
        base: text(v, "/base/sha"),
        url: text(v, "/html_url"),
        updated: unix_seconds(&text(v, "/updated_at")),
    }
}

fn review_comment(v: &Value) -> ReviewComment {
    let line = v.get("line").and_then(Value::as_u64);
    let original = v.get("original_line").and_then(Value::as_u64);
    ReviewComment {
        id: v.get("id").map(|i| i.to_string()).unwrap_or_default(),
        author: text(v, "/user/login"),
        body: text(v, "/body"),
        created: unix_seconds(&text(v, "/created_at")),
        path: v.get("path").and_then(Value::as_str).map(str::to_owned),
        line: line.or(original).map(|l| l as u32),
        field: None,
        // GitHub reports no current line when the commented line is gone from the head.
        outdated: line.is_none() && original.is_some(),
    }
}

fn issue_comment(v: &Value) -> ReviewComment {
    ReviewComment {
        id: format!(
            "issue-{}",
            v.get("id").map(|i| i.to_string()).unwrap_or_default()
        ),
        author: text(v, "/user/login"),
        body: text(v, "/body"),
        created: unix_seconds(&text(v, "/created_at")),
        path: None,
        line: None,
        field: None,
        outdated: false,
    }
}

fn check_state(status: &str, conclusion: Option<&str>) -> CiState {
    if status != "completed" {
        return CiState::Pending;
    }
    match conclusion {
        Some("success") => CiState::Success,
        Some("failure" | "timed_out" | "startup_failure" | "action_required") => CiState::Failure,
        _ => CiState::Neutral,
    }
}

fn status_state(state: &str) -> CiState {
    match state {
        "success" => CiState::Success,
        "failure" | "error" => CiState::Failure,
        "pending" => CiState::Pending,
        _ => CiState::Neutral,
    }
}

impl<T: Transport> ReviewProvider for GithubProvider<'_, T> {
    fn repo(&self) -> &ReviewRepo {
        &self.repo
    }

    fn list(&self) -> LocalBoxFuture<'_, Result<Vec<PullRequest>, ReviewError>> {
        Box::pin(async move {
            let url = self.url("/pulls?state=open&sort=updated&direction=desc&per_page=50");
            let all = self.pages(url).await?;
            let mut pulls: Vec<PullRequest> = all.iter().map(pull).collect();
            pulls.sort_by_key(|p| std::cmp::Reverse(p.updated));
            Ok(pulls)
        })
    }

    fn get(&self, number: u64) -> LocalBoxFuture<'_, Result<PullRequest, ReviewError>> {
        Box::pin(async move {
            let reply = self
                .client
                .get(&self.url(&format!("/pulls/{number}")))
                .await?;
            Ok(pull(&reply.body))
        })
    }

    fn head_ref(&self, number: u64) -> String {
        format!("refs/pull/{number}/head")
    }

    fn comments(&self, number: u64) -> LocalBoxFuture<'_, Result<Vec<ReviewComment>, ReviewError>> {
        Box::pin(async move {
            let lines = self
                .pages(self.url(&format!("/pulls/{number}/comments?per_page=100")))
                .await?;
            let general = self
                .pages(self.url(&format!("/issues/{number}/comments?per_page=100")))
                .await?;
            let mut out: Vec<ReviewComment> = lines.iter().map(review_comment).collect();
            out.extend(general.iter().map(issue_comment));
            out.sort_by_key(|c| c.created);
            Ok(out)
        })
    }

    fn comment<'a>(
        &'a self,
        pull: &'a PullRequest,
        comment: &'a NewReviewComment,
    ) -> LocalBoxFuture<'a, Result<ReviewComment, ReviewError>> {
        Box::pin(async move {
            let body = json!({
                "body": comment.body,
                "commit_id": pull.head,
                "path": comment.path,
                "line": comment.line,
                "side": "RIGHT",
            });
            let reply = self
                .client
                .post(
                    &self.url(&format!("/pulls/{}/comments", pull.number)),
                    &body,
                )
                .await?;
            Ok(review_comment(&reply.body))
        })
    }

    fn submit<'a>(
        &'a self,
        pull: &'a PullRequest,
        review: &'a ReviewSubmission,
    ) -> LocalBoxFuture<'a, Result<(), ReviewError>> {
        Box::pin(async move {
            let event = match review.verdict {
                ReviewVerdict::Approve => "APPROVE",
                ReviewVerdict::RequestChanges => "REQUEST_CHANGES",
                ReviewVerdict::Comment => "COMMENT",
            };
            let body = json!({ "commit_id": pull.head, "body": review.body, "event": event });
            self.client
                .post(&self.url(&format!("/pulls/{}/reviews", pull.number)), &body)
                .await?;
            Ok(())
        })
    }

    fn ci<'a>(
        &'a self,
        pull: &'a PullRequest,
    ) -> LocalBoxFuture<'a, Result<CiStatus, ReviewError>> {
        Box::pin(async move {
            let runs = self
                .client
                .get(&self.url(&format!("/commits/{}/check-runs?per_page=100", pull.head)))
                .await?;
            let statuses = self
                .client
                .get(&self.url(&format!("/commits/{}/status", pull.head)))
                .await?;
            let mut checks: Vec<CiCheck> = runs
                .body
                .get("check_runs")
                .and_then(Value::as_array)
                .map(|a| a.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|r| CiCheck {
                    name: text(r, "/name"),
                    state: check_state(
                        &text(r, "/status"),
                        r.get("conclusion").and_then(Value::as_str),
                    ),
                    url: r.get("html_url").and_then(Value::as_str).map(str::to_owned),
                })
                .collect();
            checks.extend(
                statuses
                    .body
                    .get("statuses")
                    .and_then(Value::as_array)
                    .map(|a| a.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .map(|s| CiCheck {
                        name: text(s, "/context"),
                        state: status_state(&text(s, "/state")),
                        url: s
                            .get("target_url")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    }),
            );
            Ok(CiStatus {
                state: combine(&checks),
                checks,
            })
        })
    }

    fn create<'a>(
        &'a self,
        pull: &'a NewPullRequest,
    ) -> LocalBoxFuture<'a, Result<PullRequest, ReviewError>> {
        Box::pin(async move {
            let body = json!({
                "title": pull.title,
                "body": pull.body,
                "head": pull.head,
                "base": pull.base,
                "draft": pull.draft,
            });
            let reply = self.client.post(&self.url("/pulls"), &body).await?;
            Ok(self::pull(&reply.body))
        })
    }
}

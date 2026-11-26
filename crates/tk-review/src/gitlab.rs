// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! GitLab (gitlab.com and self-managed): merge requests, discussions, approvals, pipelines.

use serde_json::{Value, json};
use tk_net::services::LocalBoxFuture;
use tk_net::transport::Transport;

use super::client::{Client, encode_component, items, text};
use super::time::unix_seconds;
use super::{
    CiCheck, CiState, CiStatus, NewPullRequest, NewReviewComment, PullRequest, PullState,
    ReviewComment, ReviewError, ReviewProvider, ReviewRepo, ReviewSubmission, ReviewVerdict,
    combine,
};

const MAX_PAGES: usize = 5;

pub struct GitlabProvider<'t, T: Transport> {
    client: Client<'t, T>,
    repo: ReviewRepo,
}

impl<'t, T: Transport> GitlabProvider<'t, T> {
    pub fn new(transport: &'t T, repo: ReviewRepo, token: &str, user_agent: &str) -> Self {
        let client = Client::new(transport, token, &repo.api, user_agent);
        Self { client, repo }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/projects/{}{path}",
            self.client.api,
            encode_component(&self.repo.project)
        )
    }

    /// Every item of a list endpoint, following `X-Next-Page`.
    async fn pages(&self, url: String) -> Result<Vec<Value>, ReviewError> {
        let sep = if url.contains('?') { '&' } else { '?' };
        let mut out = Vec::new();
        let mut page = 1u32;
        for _ in 0..MAX_PAGES {
            let reply = self.client.get(&format!("{url}{sep}page={page}")).await?;
            let next = reply
                .header("x-next-page")
                .and_then(|n| n.trim().parse::<u32>().ok());
            out.extend(items(reply.body));
            match next {
                Some(n) => page = n,
                None => break,
            }
        }
        Ok(out)
    }

    async fn merge_request(&self, number: u64) -> Result<Value, ReviewError> {
        Ok(self
            .client
            .get(&self.url(&format!("/merge_requests/{number}")))
            .await?
            .body)
    }
}

fn pull(v: &Value) -> PullRequest {
    let state = match v.get("state").and_then(Value::as_str) {
        Some("merged") => PullState::Merged,
        Some("closed" | "locked") => PullState::Closed,
        _ => PullState::Open,
    };
    PullRequest {
        number: v.get("iid").and_then(Value::as_u64).unwrap_or(0),
        title: text(v, "/title"),
        author: text(v, "/author/username"),
        state,
        draft: v
            .get("draft")
            .or_else(|| v.get("work_in_progress"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        source: text(v, "/source_branch"),
        target: text(v, "/target_branch"),
        head: text(v, "/sha"),
        // Only a single merge request carries `diff_refs`; the list leaves this empty.
        base: text(v, "/diff_refs/base_sha"),
        url: text(v, "/web_url"),
        updated: unix_seconds(&text(v, "/updated_at")),
    }
}

fn note<A>(v: &Value) -> ReviewComment<A> {
    let position = v.get("position").filter(|p| !p.is_null());
    let path = position
        .and_then(|p| p.get("new_path").or_else(|| p.get("old_path")))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let line = position
        .and_then(|p| p.get("new_line").and_then(Value::as_u64))
        .map(|l| l as u32);
    ReviewComment {
        id: v.get("id").map(|i| i.to_string()).unwrap_or_default(),
        author: text(v, "/author/username"),
        body: text(v, "/body"),
        created: unix_seconds(&text(v, "/created_at")),
        outdated: position.is_some() && line.is_none(),
        path,
        line,
        field: None,
    }
}

fn job_state(status: &str, allow_failure: bool) -> CiState {
    match status {
        "success" => CiState::Success,
        "failed" if allow_failure => CiState::Neutral,
        "failed" => CiState::Failure,
        "created" | "pending" | "running" | "waiting_for_resource" | "preparing" | "scheduled" => {
            CiState::Pending
        }
        _ => CiState::Neutral,
    }
}

impl<T: Transport, A> ReviewProvider<A> for GitlabProvider<'_, T> {
    fn repo(&self) -> &ReviewRepo {
        &self.repo
    }

    fn list(&self) -> LocalBoxFuture<'_, Result<Vec<PullRequest>, ReviewError>> {
        Box::pin(async move {
            let all = self
                .pages(
                    self.url(
                        "/merge_requests?state=opened&order_by=updated_at&sort=desc&per_page=50",
                    ),
                )
                .await?;
            let mut pulls: Vec<PullRequest> = all.iter().map(pull).collect();
            pulls.sort_by_key(|p| std::cmp::Reverse(p.updated));
            Ok(pulls)
        })
    }

    fn get(&self, number: u64) -> LocalBoxFuture<'_, Result<PullRequest, ReviewError>> {
        Box::pin(async move { Ok(pull(&self.merge_request(number).await?)) })
    }

    fn head_ref(&self, number: u64) -> String {
        format!("refs/merge-requests/{number}/head")
    }

    fn comments(
        &self,
        number: u64,
    ) -> LocalBoxFuture<'_, Result<Vec<ReviewComment<A>>, ReviewError>> {
        Box::pin(async move {
            let discussions = self
                .pages(self.url(&format!(
                    "/merge_requests/{number}/discussions?per_page=100"
                )))
                .await?;
            let mut out: Vec<ReviewComment<A>> = discussions
                .iter()
                .flat_map(|d| {
                    d.get("notes")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .filter(|n| !n.get("system").and_then(Value::as_bool).unwrap_or(false))
                .map(note)
                .collect();
            out.sort_by_key(|c| c.created);
            Ok(out)
        })
    }

    fn comment<'a>(
        &'a self,
        pull: &'a PullRequest,
        comment: &'a NewReviewComment,
    ) -> LocalBoxFuture<'a, Result<ReviewComment<A>, ReviewError>> {
        Box::pin(async move {
            // A line comment needs the diff's three shas, which only a single MR carries.
            let mr = self.merge_request(pull.number).await?;
            let refs = |k: &str| text(&mr, &format!("/diff_refs/{k}"));
            let body = json!({
                "body": comment.body,
                "position": {
                    "position_type": "text",
                    "base_sha": refs("base_sha"),
                    "start_sha": refs("start_sha"),
                    "head_sha": refs("head_sha"),
                    "old_path": comment.path,
                    "new_path": comment.path,
                    "new_line": comment.line,
                },
            });
            let reply = self
                .client
                .post(
                    &self.url(&format!("/merge_requests/{}/discussions", pull.number)),
                    &body,
                )
                .await?;
            let first = reply
                .body
                .pointer("/notes/0")
                .cloned()
                .unwrap_or(Value::Null);
            Ok(note(&first))
        })
    }

    fn submit<'a>(
        &'a self,
        pull: &'a PullRequest,
        review: &'a ReviewSubmission,
    ) -> LocalBoxFuture<'a, Result<(), ReviewError>> {
        Box::pin(async move {
            let base = format!("/merge_requests/{}", pull.number);
            let note = |body: &str| json!({ "body": body });
            match review.verdict {
                ReviewVerdict::Approve => {
                    self.client
                        .post(
                            &self.url(&format!("{base}/approve")),
                            &json!({ "sha": pull.head }),
                        )
                        .await?;
                    if !review.body.trim().is_empty() {
                        self.client
                            .post(&self.url(&format!("{base}/notes")), &note(&review.body))
                            .await?;
                    }
                }
                ReviewVerdict::RequestChanges => {
                    // GitLab's REST API has no "request changes": withdraw an approval and
                    // leave the reason as a note.
                    self.client
                        .post(&self.url(&format!("{base}/unapprove")), &json!({}))
                        .await
                        .or_else(|e| match e {
                            // Nothing to withdraw.
                            ReviewError::NotFound(_) => Ok(super::client::Reply {
                                headers: Vec::new(),
                                body: Value::Null,
                            }),
                            other => Err(other),
                        })?;
                    let text = if review.body.trim().is_empty() {
                        "Changes requested."
                    } else {
                        review.body.as_str()
                    };
                    self.client
                        .post(&self.url(&format!("{base}/notes")), &note(text))
                        .await?;
                }
                ReviewVerdict::Comment => {
                    if review.body.trim().is_empty() {
                        return Err(ReviewError::Host("a comment needs some text".to_owned()));
                    }
                    self.client
                        .post(&self.url(&format!("{base}/notes")), &note(&review.body))
                        .await?;
                }
            }
            Ok(())
        })
    }

    fn ci<'a>(
        &'a self,
        pull: &'a PullRequest,
    ) -> LocalBoxFuture<'a, Result<CiStatus, ReviewError>> {
        Box::pin(async move {
            let pipelines = self
                .client
                .get(&self.url(&format!("/merge_requests/{}/pipelines", pull.number)))
                .await?;
            // Newest first.
            let Some(id) = items(pipelines.body)
                .first()
                .and_then(|p| p.get("id").and_then(Value::as_u64))
            else {
                return Ok(CiStatus {
                    state: CiState::None,
                    checks: Vec::new(),
                });
            };
            let jobs = self
                .pages(self.url(&format!("/pipelines/{id}/jobs?per_page=100")))
                .await?;
            let checks: Vec<CiCheck> = jobs
                .iter()
                .map(|j| CiCheck {
                    name: text(j, "/name"),
                    state: job_state(
                        &text(j, "/status"),
                        j.get("allow_failure")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    ),
                    url: j.get("web_url").and_then(Value::as_str).map(str::to_owned),
                })
                .collect();
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
            // GitLab has no draft flag on create: a "Draft:" title prefix is how it marks one.
            let title = if pull.draft && !is_draft_title(&pull.title) {
                format!("Draft: {}", pull.title)
            } else {
                pull.title.clone()
            };
            let body = json!({
                "title": title,
                "description": pull.body,
                "source_branch": pull.head,
                "target_branch": pull.base,
            });
            let reply = self
                .client
                .post(&self.url("/merge_requests"), &body)
                .await?;
            Ok(self::pull(&reply.body))
        })
    }
}

fn is_draft_title(title: &str) -> bool {
    let t = title.trim_start().to_ascii_lowercase();
    t.starts_with("draft:") || t.starts_with("[draft]") || t.starts_with("(draft)")
}

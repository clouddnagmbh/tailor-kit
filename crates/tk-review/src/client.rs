// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! One authenticated JSON call to a Git host over the app's `Transport`, shared by both
//! providers. The token only ever travels in the `Authorization` header, never follows a
//! redirect and is masked in every error.

use std::time::Duration;

use bytes::Bytes;
use serde_json::Value;
use tk_net::transport::{HttpBody, HttpRequest, SendOptions, Transport, collect};

use super::ReviewError;
use tk_secrets::redact;

const TIMEOUT: Duration = Duration::from_secs(30);
const ANSWER_LIMIT: usize = 16 * 1024 * 1024;

/// A 2xx answer.
pub(super) struct Reply {
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub(super) struct Client<'t, T: Transport> {
    transport: &'t T,
    token: String,
    /// API base without a trailing slash.
    pub api: String,
    extra: Vec<(String, String)>,
    user_agent: String,
}

fn options() -> SendOptions {
    SendOptions {
        timeout: Some(TIMEOUT),
        // A redirect could carry the token to another host.
        follow_redirects: false,
        max_redirects: 0,
        client_identity: None,
    }
}

/// `message` of an error body (GitHub, GitLab), with GitHub's `errors` details.
fn error_text(body: &Result<Value, String>) -> String {
    let text = match body {
        Ok(v) => {
            let mut s = v
                .get("message")
                .or_else(|| v.get("error_description"))
                .or_else(|| v.get("error"))
                .and_then(|m| match m {
                    Value::String(s) => Some(s.clone()),
                    Value::Null => None,
                    other => Some(other.to_string()),
                })
                .unwrap_or_else(|| v.to_string());
            if let Some(errors) = v.get("errors").and_then(Value::as_array) {
                let details: Vec<String> = errors
                    .iter()
                    .filter_map(|e| match e {
                        Value::String(s) => Some(s.clone()),
                        other => other
                            .get("message")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or_else(|| {
                                let field = other.get("field").and_then(Value::as_str)?;
                                let code = other.get("code").and_then(Value::as_str)?;
                                Some(format!("{field} {code}"))
                            }),
                    })
                    .collect();
                if !details.is_empty() {
                    s = format!("{s} ({})", details.join("; "));
                }
            }
            s
        }
        Err(text) => text.clone(),
    };
    text.trim().chars().take(300).collect()
}

impl<'t, T: Transport> Client<'t, T> {
    pub fn new(transport: &'t T, token: &str, api: &str, user_agent: &str) -> Self {
        Self {
            transport,
            token: token.trim().to_owned(),
            api: api.trim().trim_end_matches('/').to_owned(),
            extra: Vec::new(),
            user_agent: user_agent.to_owned(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.extra.push((name.to_owned(), value.to_owned()));
        self
    }

    fn mask(&self, text: &str) -> String {
        redact(text, std::slice::from_ref(&self.token))
    }

    /// `method url` with an optional JSON body.
    pub async fn call(
        &self,
        method: &str,
        url: &str,
        body: Option<&Value>,
    ) -> Result<Reply, ReviewError> {
        let mut headers = vec![
            ("authorization".to_owned(), format!("Bearer {}", self.token)),
            ("accept".to_owned(), "application/json".to_owned()),
        ];
        headers.extend(self.extra.iter().cloned());
        if !self.transport_is_browser() {
            headers.push(("user-agent".to_owned(), self.user_agent.clone()));
        }
        let request = HttpRequest {
            method: method.to_owned(),
            url: url.to_owned(),
            headers,
            body: body.map(|b| HttpBody {
                content_type: Some("application/json".to_owned()),
                bytes: Bytes::from(serde_json::to_vec(b).unwrap_or_default()),
            }),
            options: options(),
        };
        let started = web_time::Instant::now();
        let transport_err = |e: String| ReviewError::Transport(self.mask(&e));
        let response = self
            .transport
            .send(request)
            .await
            .map_err(|e| transport_err(e.to_string()))?;
        let done = collect(response, ANSWER_LIMIT, started)
            .await
            .map_err(|e| transport_err(e.to_string()))?;
        let status = done.head.status;
        let text = String::from_utf8_lossy(&done.body).into_owned();
        let parsed = if text.trim().is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str::<Value>(&text).map_err(|_| text)
        };
        if (200..300).contains(&status) {
            return match parsed {
                Ok(body) => Ok(Reply {
                    headers: done.head.headers,
                    body,
                }),
                Err(_) => Err(ReviewError::Host(self.mask(&format!(
                    "{status} the answer is not JSON (is the API address right?)"
                )))),
            };
        }
        let message = self.mask(&error_text(&parsed));
        Err(match status {
            401 => ReviewError::Auth(format!("401 {message}")),
            403 if !message.to_lowercase().contains("rate limit") => {
                ReviewError::Auth(format!("403 {message}"))
            }
            404 => ReviewError::NotFound(message),
            _ => ReviewError::Host(format!("{status} {message}")),
        })
    }

    /// The browser's `fetch` refuses a `User-Agent` of its own; natively it is polite to send one.
    fn transport_is_browser(&self) -> bool {
        cfg!(target_arch = "wasm32")
    }

    pub async fn get(&self, url: &str) -> Result<Reply, ReviewError> {
        self.call("GET", url, None).await
    }

    pub async fn post(&self, url: &str, body: &Value) -> Result<Reply, ReviewError> {
        self.call("POST", url, Some(body)).await
    }

    /// Whether `url` is on the API's own origin: only then may the token go there.
    pub fn same_origin(&self, url: &str) -> bool {
        match (url::Url::parse(&self.api), url::Url::parse(url)) {
            (Ok(a), Ok(b)) => a.origin() == b.origin(),
            _ => false,
        }
    }
}

/// The array in `body` (a non-array answer is empty).
pub(super) fn items(body: Value) -> Vec<Value> {
    match body {
        Value::Array(a) => a,
        _ => Vec::new(),
    }
}

pub(super) fn text(v: &Value, pointer: &str) -> String {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Percent-encodes everything but unreserved characters (a GitLab project path in a URL).
pub(super) fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

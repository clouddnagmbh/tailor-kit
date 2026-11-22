// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Device-code login (RFC 8628) for `github.com` and `gitlab.com`, and only if the project
//! lead registered an OAuth app: [`client_id`] is empty until then and the UI offers just the
//! token. The shells run the polling loop (sleeping is theirs: a timer natively, `setTimeout`
//! in the browser); the browser needs the companion, as the hosts' login endpoints allow no
//! CORS.

use bytes::Bytes;
use rt_core::services::DeviceCodePrompt;
use rt_core::transport::{HttpBody, HttpRequest, SendOptions, Transport, collect};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ReviewError, ReviewKind, ReviewRepo};
use crate::ai::redact;

/// Client id of the RequestTailor OAuth app on github.com. Public by design. Empty until the
/// project lead has registered the app.
pub const GITHUB_CLIENT_ID: &str = "";
/// Client id of the RequestTailor OAuth application on gitlab.com. Empty until registered.
pub const GITLAB_CLIENT_ID: &str = "";

/// The OAuth app's client id for `repo`'s host: only `github.com` and `gitlab.com`, and only
/// once registered. `None`: device-code login is not offered.
pub fn client_id(repo: &ReviewRepo) -> Option<&'static str> {
    let id = match (repo.kind, repo.host.as_str()) {
        (ReviewKind::Github, "github.com") => GITHUB_CLIENT_ID,
        (ReviewKind::Gitlab, "gitlab.com") => GITLAB_CLIENT_ID,
        _ => "",
    };
    (!id.is_empty()).then_some(id)
}

/// A started login: what the user sees and what the polling needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceLogin {
    pub prompt: DeviceCodePrompt,
    pub device_code: String,
    /// Seconds between polls.
    pub interval: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevicePoll {
    /// The user has not finished yet; poll again after the interval.
    Pending,
    /// Poll more slowly: the interval is now this many seconds.
    SlowDown(u64),
    Token(String),
}

fn web_base(repo: &ReviewRepo) -> String {
    let scheme = if repo.api.starts_with("http://") {
        "http"
    } else {
        "https"
    };
    format!("{scheme}://{}", repo.host)
}

fn form(fields: &[(&str, &str)]) -> Bytes {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in fields {
        s.append_pair(k, v);
    }
    Bytes::from(s.finish())
}

async fn post_form<T: Transport>(
    transport: &T,
    url: String,
    fields: &[(&str, &str)],
) -> Result<(u16, Value), ReviewError> {
    let request = HttpRequest {
        method: "POST".into(),
        url,
        headers: vec![("accept".into(), "application/json".into())],
        body: Some(HttpBody {
            content_type: Some("application/x-www-form-urlencoded".into()),
            bytes: form(fields),
        }),
        options: SendOptions {
            timeout: Some(std::time::Duration::from_secs(30)),
            follow_redirects: false,
            max_redirects: 0,
            client_identity: None,
        },
    };
    let started = web_time::Instant::now();
    let failed = |e: String| ReviewError::Transport(e);
    let response = transport
        .send(request)
        .await
        .map_err(|e| failed(e.to_string()))?;
    let done = collect(response, 1024 * 1024, started)
        .await
        .map_err(|e| failed(e.to_string()))?;
    let body = serde_json::from_slice::<Value>(&done.body).unwrap_or(Value::Null);
    Ok((done.head.status, body))
}

fn message(body: &Value) -> String {
    body.get("error_description")
        .or_else(|| body.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("the login was refused")
        .chars()
        .take(200)
        .collect()
}

/// Asks the host for a device code.
pub async fn start<T: Transport>(
    transport: &T,
    repo: &ReviewRepo,
    client_id: &str,
) -> Result<DeviceLogin, ReviewError> {
    let (url, scope) = match repo.kind {
        ReviewKind::Github => (format!("{}/login/device/code", web_base(repo)), "repo"),
        ReviewKind::Gitlab => (format!("{}/oauth/authorize_device", web_base(repo)), "api"),
    };
    let (status, body) = post_form(
        transport,
        url,
        &[("client_id", client_id), ("scope", scope)],
    )
    .await?;
    if !(200..300).contains(&status) {
        return Err(ReviewError::Host(format!("{status} {}", message(&body))));
    }
    let text = |k: &str| body.get(k).and_then(Value::as_str).map(str::to_owned);
    let (Some(device_code), Some(user_code), Some(uri)) = (
        text("device_code"),
        text("user_code"),
        text("verification_uri"),
    ) else {
        return Err(ReviewError::Host(
            "the host did not answer with a device code".into(),
        ));
    };
    Ok(DeviceLogin {
        prompt: DeviceCodePrompt {
            user_code,
            verification_uri: uri,
            verification_uri_complete: text("verification_uri_complete"),
            expires_in: body
                .get("expires_in")
                .and_then(Value::as_u64)
                .unwrap_or(900),
        },
        device_code,
        interval: body
            .get("interval")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .max(1),
    })
}

/// One poll. Errors end the login (denied, expired, refused); the token is masked in them.
pub async fn poll<T: Transport>(
    transport: &T,
    repo: &ReviewRepo,
    client_id: &str,
    login: &DeviceLogin,
) -> Result<DevicePoll, ReviewError> {
    let url = match repo.kind {
        ReviewKind::Github => format!("{}/login/oauth/access_token", web_base(repo)),
        ReviewKind::Gitlab => format!("{}/oauth/token", web_base(repo)),
    };
    let (_, body) = post_form(
        transport,
        url,
        &[
            ("client_id", client_id),
            ("device_code", &login.device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ],
    )
    .await?;
    if let Some(token) = body.get("access_token").and_then(Value::as_str) {
        return Ok(DevicePoll::Token(token.to_owned()));
    }
    let error = redact(&message(&body), &[login.device_code.clone()]);
    match body.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => Ok(DevicePoll::Pending),
        Some("slow_down") => Ok(DevicePoll::SlowDown(login.interval + 5)),
        Some("expired_token") => Err(ReviewError::Auth(
            "the code expired; start the login again".into(),
        )),
        Some("access_denied") => Err(ReviewError::Auth("the login was denied".into())),
        _ => Err(ReviewError::Auth(error)),
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The providers, over the app's own `Transport`. None is preferred; the user picks one and
//! gives its key (or a local endpoint).
//!
//! | kind | module | wire format |
//! | --- | --- | --- |
//! | `anthropic` | `anthropic` | Messages API (`/v1/messages`) |
//! | `openai`, `azure-openai`, `mistral`, `openai-compatible` | `chat` | chat completions |
//! | `gemini` | `gemini` | `generateContent` / `streamGenerateContent?alt=sse` |
//! | `bedrock` | (later) | Converse; its streaming is AWS event stream, not SSE |
//!
//! Every provider can answer once ([`AiProvider::reply`], with tool calls and a JSON schema
//! where the API has one) or stream ([`AiProvider::stream`]). Redirects are never followed
//! (the key could go to another host), the answer is capped at 8 MiB, a call times out after
//! 300 s, and every error is masked with the key.

use std::time::Duration;

use bytes::Bytes;
use serde_json::{Value, json};
use tk_net::LocalBoxFuture;
use tk_net::transport::{HttpBody, HttpRequest, SendOptions, Transport, collect};

use crate::{
    AiCall, AiConfig, AiDelta, AiError, AiProvider, AiProviderKind, AiReply, AiRole, AiStop, redact,
};

mod anthropic;
mod chat;
mod gemini;
mod sse;

pub use anthropic::{ANTHROPIC_BASE_URL, ANTHROPIC_VERSION, AnthropicProvider};
pub use chat::{ChatProvider, OpenAiCompatibleProvider};
pub use gemini::{GEMINI_BASE_URL, GeminiProvider};

/// A model call may think for a while.
const TIMEOUT: Duration = Duration::from_secs(300);
/// Largest answer read.
const ANSWER_LIMIT: usize = 8 * 1024 * 1024;

fn options() -> SendOptions {
    SendOptions {
        timeout: Some(TIMEOUT),
        // A redirect could carry the key to another host.
        follow_redirects: false,
        max_redirects: 0,
        client_identity: None,
    }
}

fn trim_base(base: &str) -> &str {
    base.trim().trim_end_matches('/')
}

/// The base URL of `config`, or `default`.
fn base_url(config: &AiConfig, default: &str) -> String {
    match config.base_url.as_deref().map(trim_base) {
        Some(b) if !b.is_empty() => b.to_owned(),
        _ => default.to_owned(),
    }
}

fn json_request(url: String, mut headers: Vec<(String, String)>, body: &Value) -> HttpRequest {
    headers.push(("accept".into(), "application/json".into()));
    HttpRequest {
        method: "POST".into(),
        url,
        headers,
        body: Some(HttpBody {
            content_type: Some("application/json".into()),
            bytes: Bytes::from(serde_json::to_vec(body).unwrap_or_default()),
        }),
        options: options(),
    }
}

fn role(r: AiRole) -> &'static str {
    match r {
        AiRole::User => "user",
        AiRole::Assistant => "assistant",
    }
}

/// `status: message` of an error answer: `error.message` (Anthropic, OpenAI, Gemini),
/// `error` as a string (Ollama), `message` (Mistral, Bedrock), else the start of the body.
fn error_message(status: u16, body: Result<Value, String>, key: &str) -> AiError {
    let message = match &body {
        Ok(v) => v
            .pointer("/error/message")
            .and_then(Value::as_str)
            .or_else(|| v.get("error").and_then(Value::as_str))
            .or_else(|| v.get("message").and_then(Value::as_str))
            .map(str::to_owned)
            .unwrap_or_else(|| v.to_string()),
        Err(text) => text.clone(),
    };
    let message: String = message.trim().chars().take(300).collect();
    AiError::Provider(redact(&format!("{status} {message}"), &[key.to_owned()]))
}

/// Scheme, host and path of `url`: what the preview shows. Never a key, a query or user info.
pub fn endpoint_of(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => {
            let host = u.host_str().unwrap_or_default();
            let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
            format!("{}://{host}{port}{}", u.scheme(), u.path())
        }
        Err(_) => String::new(),
    }
}

/// The URL a call of `config` goes to (what [`endpoint_of`] shortens for the preview).
pub fn request_url(config: &AiConfig, stream: bool) -> Result<String, AiError> {
    config.validate()?;
    Ok(match config.provider {
        AiProviderKind::Anthropic => anthropic::url(config),
        AiProviderKind::Gemini => gemini::url(config, stream),
        AiProviderKind::Bedrock => return Err(bedrock_later()),
        _ => chat::url(config),
    })
}

fn bedrock_later() -> AiError {
    AiError::Unsupported(
        "AWS Bedrock arrives later (its streaming is not server-sent events)".into(),
    )
}

/// One request/answer wire format.
pub trait Wire {
    /// The HTTP request; `stream`: ask for server-sent events.
    fn request(&self, call: &AiCall, stream: bool) -> HttpRequest;
    /// The key, for masking errors.
    fn key(&self) -> &str;
    /// The answer of a non-streaming call.
    fn parse(&self, body: &Value) -> Result<AiReply, AiError>;
    fn decoder(&self) -> Box<dyn Decoder>;
    /// Whether an error answer to a call with tools means "this server has no tool use".
    fn tools_refused(&self, _status: u16) -> bool {
        false
    }
}

/// Turns the bytes of a streamed answer into pieces and, at the end, the whole reply.
pub trait Decoder {
    fn feed(&mut self, chunk: &[u8], out: &mut Vec<AiDelta>) -> Result<(), AiError>;
    fn finish(self: Box<Self>, out: &mut Vec<AiDelta>) -> Result<AiReply, AiError>;
}

/// A provider over a [`Wire`].
pub struct Wired<'t, T: Transport, W> {
    transport: &'t T,
    wire: W,
}

impl<T: Transport, W: Wire> Wired<'_, T, W> {
    fn masked(&self, text: String) -> String {
        redact(&text, &[self.wire.key().to_owned()])
    }

    async fn send(&self, request: HttpRequest) -> Result<tk_net::transport::Response, AiError> {
        self.transport
            .send(request)
            .await
            .map_err(|e| AiError::Transport(self.masked(e.to_string())))
    }

    async fn refused(&self, response: tk_net::transport::Response, call: &AiCall) -> AiError {
        let status = response.head.status;
        let started = web_time::Instant::now();
        let text = match collect(response, ANSWER_LIMIT, started).await {
            Ok(done) => String::from_utf8_lossy(&done.body).into_owned(),
            Err(e) => return AiError::Transport(self.masked(e.to_string())),
        };
        if !call.tools.is_empty() && self.wire.tools_refused(status) {
            return AiError::Unsupported("this server does not support tool use".into());
        }
        let body = serde_json::from_str::<Value>(&text).map_err(|_| self.masked(text));
        error_message(status, body, self.wire.key())
    }

    async fn once(&self, call: &AiCall) -> Result<AiReply, AiError> {
        let response = self.send(self.wire.request(call, false)).await?;
        if response.head.status >= 400 {
            return Err(self.refused(response, call).await);
        }
        let started = web_time::Instant::now();
        let done = collect(response, ANSWER_LIMIT, started)
            .await
            .map_err(|e| AiError::Transport(self.masked(e.to_string())))?;
        let body = serde_json::from_slice::<Value>(&done.body)
            .map_err(|_| AiError::Answer("the answer was not JSON".into()))?;
        self.wire.parse(&body)
    }

    async fn streamed(&self, call: &AiCall, on: &dyn Fn(AiDelta)) -> Result<AiReply, AiError> {
        let response = self.send(self.wire.request(call, true)).await?;
        if response.head.status >= 400 {
            return Err(self.refused(response, call).await);
        }
        let content_type = response
            .head
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.to_ascii_lowercase());
        // A server that ignored `stream: true` answers with plain JSON.
        if content_type
            .as_deref()
            .is_some_and(|c| c.contains("json") && !c.contains("event-stream"))
        {
            let started = web_time::Instant::now();
            let done = collect(response, ANSWER_LIMIT, started)
                .await
                .map_err(|e| AiError::Transport(self.masked(e.to_string())))?;
            let body = serde_json::from_slice::<Value>(&done.body)
                .map_err(|_| AiError::Answer("the answer was not JSON".into()))?;
            let reply = self.wire.parse(&body)?;
            if !reply.text.is_empty() {
                on(AiDelta::Text {
                    text: reply.text.clone(),
                });
            }
            for c in &reply.tool_calls {
                on(AiDelta::ToolCall { call: c.clone() });
            }
            return Ok(reply);
        }
        let mut body = response.body;
        let mut decoder = self.wire.decoder();
        let mut out = Vec::new();
        let mut total = 0usize;
        loop {
            let next =
                std::future::poll_fn(|cx| futures_core::Stream::poll_next(body.as_mut(), cx)).await;
            match next {
                None => break,
                Some(Err(e)) => return Err(AiError::Transport(self.masked(e.to_string()))),
                Some(Ok(chunk)) => {
                    total += chunk.len();
                    if total > ANSWER_LIMIT {
                        return Err(AiError::Answer("the answer is too large".into()));
                    }
                    decoder.feed(&chunk, &mut out)?;
                    for d in out.drain(..) {
                        on(d);
                    }
                }
            }
        }
        let reply = decoder.finish(&mut out)?;
        for d in out.drain(..) {
            on(d);
        }
        Ok(reply)
    }
}

/// The text of `reply` as `complete` returns it; refusals, cut-off and empty answers are
/// errors there.
pub fn reply_text(reply: AiReply) -> Result<String, AiError> {
    match reply.stop {
        AiStop::Refusal => Err(AiError::Provider("the model declined to answer".into())),
        AiStop::MaxTokens => Err(AiError::Answer(
            "the answer was cut off at the token limit".into(),
        )),
        _ if reply.text.trim().is_empty() => Err(AiError::Answer("the answer had no text".into())),
        _ => Ok(reply.text),
    }
}

impl<T: Transport, W: Wire> AiProvider for Wired<'_, T, W> {
    fn complete<'a>(&'a self, call: AiCall) -> LocalBoxFuture<'a, Result<String, AiError>> {
        Box::pin(async move { reply_text(self.once(&call).await?) })
    }

    fn reply<'a>(&'a self, call: AiCall) -> LocalBoxFuture<'a, Result<AiReply, AiError>> {
        Box::pin(async move { self.once(&call).await })
    }

    fn stream<'a>(
        &'a self,
        call: AiCall,
        on: &'a dyn Fn(AiDelta),
    ) -> LocalBoxFuture<'a, Result<AiReply, AiError>> {
        Box::pin(async move { self.streamed(&call, on).await })
    }
}

/// Appends the schema to the system prompt, for providers without a schema mode.
fn schema_in_prompt(system: &str, schema: &Value) -> String {
    format!(
        "{system}\n\nAnswer with one JSON value that follows this JSON Schema, and nothing else:\n{schema}"
    )
}

/// Whether `schema` meets the strict mode of OpenAI-style structured outputs (every object
/// closed and all its properties required).
fn strict_compatible(schema: &Value) -> bool {
    match schema {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("object") {
                let props = map.get("properties").and_then(Value::as_object);
                let required: Vec<&str> = map
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let closed = map.get("additionalProperties") == Some(&json!(false));
                let all = props.is_none_or(|p| p.keys().all(|k| required.contains(&k.as_str())));
                if !closed || !all {
                    return false;
                }
            }
            map.values().all(strict_compatible)
        }
        Value::Array(items) => items.iter().all(strict_compatible),
        _ => true,
    }
}

/// The provider `config` names, over `transport`. `browser`: called from a web page with
/// `fetch` (Anthropic needs a header for that).
pub fn provider<'t, T: Transport>(
    transport: &'t T,
    config: &AiConfig,
    key: Option<String>,
    browser: bool,
) -> Result<Box<dyn AiProvider + 't>, AiError> {
    config.validate()?;
    Ok(match config.provider {
        AiProviderKind::Anthropic => {
            let p = AnthropicProvider::new(transport, config, key)?;
            Box::new(if browser { p.in_browser() } else { p })
        }
        AiProviderKind::Gemini => Box::new(GeminiProvider::new(transport, config, key)?),
        AiProviderKind::Bedrock => return Err(bedrock_later()),
        _ => Box::new(ChatProvider::new(transport, config, key)?),
    })
}

#[cfg(test)]
pub(super) mod testing {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use tk_net::transport::{HttpVersion, Response, ResponseHead, TransportError};

    use super::*;

    struct Chunks(VecDeque<Bytes>);

    impl futures_core::Stream for Chunks {
        type Item = Result<Bytes, TransportError>;
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(self.0.pop_front().map(Ok))
        }
    }

    type Answer = (u16, Vec<Bytes>, Option<&'static str>);

    /// Answers requests in order (the last answer repeats), keeping the requests.
    pub struct Fake {
        answers: RefCell<VecDeque<Answer>>,
        pub seen: RefCell<Vec<HttpRequest>>,
    }

    impl Fake {
        pub fn new(status: u16, body: &str) -> Self {
            Self::chunks(status, &[body], None)
        }
        pub fn chunks(status: u16, chunks: &[&str], content_type: Option<&'static str>) -> Self {
            Self {
                answers: RefCell::new(VecDeque::from([(
                    status,
                    chunks.iter().map(|c| Bytes::from(c.to_string())).collect(),
                    content_type,
                )])),
                seen: RefCell::new(Vec::new()),
            }
        }
        pub fn sent_json(&self) -> Value {
            let r = &self.seen.borrow()[0];
            serde_json::from_slice(&r.body.as_ref().unwrap().bytes).unwrap()
        }
        pub fn header(&self, name: &str) -> Option<String> {
            self.seen.borrow()[0]
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        }
    }

    impl Transport for Fake {
        async fn send(&self, request: HttpRequest) -> Result<Response, TransportError> {
            let url = request.url.clone();
            assert!(!request.options.follow_redirects, "redirects must stay off");
            self.seen.borrow_mut().push(request);
            let (status, chunks, ct) = {
                let mut a = self.answers.borrow_mut();
                if a.len() > 1 {
                    a.pop_front().unwrap()
                } else {
                    a.front().unwrap().clone()
                }
            };
            Ok(Response {
                head: ResponseHead {
                    status,
                    version: HttpVersion::Http11,
                    headers: ct
                        .map(|c| vec![("content-type".to_owned(), c.to_owned())])
                        .unwrap_or_default(),
                    url,
                    redirects: vec![],
                    timing: Default::default(),
                },
                body: Box::pin(Chunks(chunks.into())),
            })
        }
    }

    pub fn call() -> AiCall {
        AiCall {
            system: "sys".into(),
            messages: vec![crate::AiMessage::text(AiRole::User, "hi")],
            max_tokens: 100,
            ..Default::default()
        }
    }

    pub fn config(provider: AiProviderKind, model: &str, base: Option<&str>) -> AiConfig {
        AiConfig {
            provider,
            model: model.into(),
            base_url: base.map(str::to_owned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_show_no_query_or_user_info() {
        assert_eq!(
            endpoint_of("https://u:p@api.example.com:8443/v1/chat?key=abc#x"),
            "https://api.example.com:8443/v1/chat"
        );
        assert_eq!(endpoint_of("not a url"), "");
    }

    #[test]
    fn strict_schemas() {
        assert!(strict_compatible(
            &json!({"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"additionalProperties":false})
        ));
        assert!(!strict_compatible(
            &json!({"type":"object","properties":{"a":{"type":"string"}}})
        ));
    }
}

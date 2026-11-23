// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The Messages API (`POST /v1/messages`, headers `x-api-key` and `anthropic-version`).
//!
//! For current models it asks for adaptive thinking and medium effort; on Anthropic's own
//! endpoint, for the models that support it, it turns on the server-side fallback when a
//! model declines. Called from the browser with `fetch`, it sends
//! `anthropic-dangerous-direct-browser-access: true`. Tools are `tools[].input_schema`
//! (answer: `tool_use` blocks; result: `tool_result` blocks first in the user message), a JSON
//! schema is `output_config.format`, streaming is SSE with `content_block_delta` events.

use std::collections::BTreeMap;

use rt_core::transport::{HttpRequest, Transport};
use serde_json::{Value, json};

use super::sse::{Sse, SseEvent};
use super::{Decoder, Wire, Wired, base_url, json_request, role};
use crate::ai::{
    AiCall, AiConfig, AiDelta, AiError, AiMessage, AiReply, AiStop, AiToolCall, AiUsage,
};

/// Anthropic's public endpoint.
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
/// The `anthropic-version` header.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Models with adaptive thinking and `output_config.effort`.
const ADAPTIVE: &[&str] = &[
    "claude-fable-",
    "claude-mythos-",
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-haiku-5",
    "claude-opus-4-6",
    "claude-opus-4-7",
    "claude-opus-4-8",
    "claude-sonnet-4-6",
];
/// Models with the server-side refusal fallback.
const FALLBACKS: &[&str] = &[
    "claude-fable-5-2",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];

fn base(config: &AiConfig) -> String {
    let base = base_url(config, ANTHROPIC_BASE_URL);
    // `/v1/messages` is appended; a base that already ends in `/v1` is fine too.
    base.strip_suffix("/v1").unwrap_or(&base).to_owned()
}

pub(super) fn url(config: &AiConfig) -> String {
    format!("{}/v1/messages", base(config))
}

pub struct Anthropic {
    key: String,
    model: String,
    base: String,
    browser: bool,
}

/// The Messages API.
pub type AnthropicProvider<'t, T> = Wired<'t, T, Anthropic>;

impl<'t, T: Transport> Wired<'t, T, Anthropic> {
    /// `key` is required ([`AiError::NoKey`]).
    pub fn new(transport: &'t T, config: &AiConfig, key: Option<String>) -> Result<Self, AiError> {
        let key = key.filter(|k| !k.trim().is_empty()).ok_or(AiError::NoKey)?;
        Ok(Self {
            transport,
            wire: Anthropic {
                key: key.trim().to_owned(),
                model: config.model.trim().to_owned(),
                base: base(config),
                browser: false,
            },
        })
    }

    /// Calls straight from a web page (`fetch`): adds the direct browser access header.
    pub fn in_browser(mut self) -> Self {
        self.wire.browser = true;
        self
    }

    /// The HTTP request for `call` (not streaming).
    pub fn request(&self, call: &AiCall) -> HttpRequest {
        self.wire.request(call, false)
    }
}

fn message(m: &AiMessage) -> Value {
    if m.tool_calls.is_empty() && m.tool_results.is_empty() {
        return json!({ "role": role(m.role), "content": m.content });
    }
    let mut blocks = Vec::new();
    for r in &m.tool_results {
        let mut b =
            json!({ "type": "tool_result", "tool_use_id": r.call_id, "content": r.content });
        if r.is_error {
            b["is_error"] = json!(true);
        }
        blocks.push(b);
    }
    if !m.content.is_empty() {
        blocks.push(json!({ "type": "text", "text": m.content }));
    }
    for c in &m.tool_calls {
        blocks.push(json!({ "type": "tool_use", "id": c.id, "name": c.name, "input": c.input }));
    }
    json!({ "role": role(m.role), "content": blocks })
}

impl Anthropic {
    fn adaptive(&self) -> bool {
        ADAPTIVE.iter().any(|p| self.model.starts_with(p))
    }

    fn fallbacks(&self) -> bool {
        self.base == ANTHROPIC_BASE_URL && FALLBACKS.contains(&self.model.as_str())
    }
}

fn stop_of(reason: Option<&str>, tools: bool) -> AiStop {
    match reason {
        Some("max_tokens") => AiStop::MaxTokens,
        Some("tool_use") => AiStop::ToolUse,
        Some("refusal") => AiStop::Refusal,
        _ if tools => AiStop::ToolUse,
        _ => AiStop::End,
    }
}

impl Wire for Anthropic {
    fn request(&self, call: &AiCall, stream: bool) -> HttpRequest {
        let mut body = json!({
            "model": self.model,
            "max_tokens": call.max_tokens,
            "system": call.system,
            "messages": call.messages.iter().map(message).collect::<Vec<_>>(),
        });
        if stream {
            body["stream"] = json!(true);
        }
        if !call.tools.is_empty() {
            body["tools"] = call
                .tools
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.input_schema }))
                .collect();
        }
        let mut output = serde_json::Map::new();
        if self.adaptive() {
            body["thinking"] = json!({ "type": "adaptive" });
            output.insert("effort".into(), json!("medium"));
        }
        if let Some(schema) = &call.json {
            output.insert(
                "format".into(),
                json!({ "type": "json_schema", "schema": schema }),
            );
        }
        if !output.is_empty() {
            body["output_config"] = Value::Object(output);
        }
        let mut headers = vec![
            ("x-api-key".to_owned(), self.key.clone()),
            ("anthropic-version".to_owned(), ANTHROPIC_VERSION.to_owned()),
        ];
        if self.fallbacks() {
            body["fallbacks"] = json!("default");
            headers.push(("anthropic-beta".into(), FALLBACK_BETA.into()));
        }
        if self.browser {
            headers.push((
                "anthropic-dangerous-direct-browser-access".into(),
                "true".into(),
            ));
        }
        json_request(format!("{}/v1/messages", self.base), headers, &body)
    }

    fn key(&self) -> &str {
        &self.key
    }

    fn parse(&self, body: &Value) -> Result<AiReply, AiError> {
        let mut text = Vec::new();
        let mut tool_calls = Vec::new();
        for b in body
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match b.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(t) = b.get("text").and_then(Value::as_str) {
                        text.push(t);
                    }
                }
                Some("tool_use") => tool_calls.push(AiToolCall {
                    id: b
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    name: b
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    input: b.get("input").cloned().unwrap_or_else(|| json!({})),
                }),
                _ => {}
            }
        }
        let usage = body.get("usage").map(|u| AiUsage {
            input_tokens: u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
            output_tokens: u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
        });
        let stop = stop_of(
            body.get("stop_reason").and_then(Value::as_str),
            !tool_calls.is_empty(),
        );
        Ok(AiReply {
            text: text.join("\n"),
            tool_calls,
            stop,
            usage,
        })
    }

    fn decoder(&self) -> Box<dyn Decoder> {
        Box::new(AnthropicDecoder::default())
    }
}

#[derive(Default)]
struct Block {
    tool: Option<(String, String)>,
    json: String,
}

#[derive(Default)]
struct AnthropicDecoder {
    sse: Sse,
    texts: Vec<String>,
    blocks: BTreeMap<u64, Block>,
    calls: Vec<AiToolCall>,
    stop: Option<String>,
    usage: AiUsage,
    seen_usage: bool,
}

impl AnthropicDecoder {
    fn event(&mut self, e: SseEvent, out: &mut Vec<AiDelta>) -> Result<(), AiError> {
        let Ok(v) = serde_json::from_str::<Value>(&e.data) else {
            return Ok(());
        };
        let index = v.get("index").and_then(Value::as_u64).unwrap_or(0);
        match v.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                if let Some(u) = v.pointer("/message/usage") {
                    self.usage.input_tokens =
                        u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                    self.seen_usage = true;
                }
            }
            Some("content_block_start") => {
                let b = v.get("content_block").cloned().unwrap_or(Value::Null);
                match b.get("type").and_then(Value::as_str) {
                    Some("tool_use") => {
                        let s = |k| {
                            b.get(k)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned()
                        };
                        self.blocks.insert(
                            index,
                            Block {
                                tool: Some((s("id"), s("name"))),
                                json: String::new(),
                            },
                        );
                    }
                    Some("text") => {
                        self.texts.push(
                            b.get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                        );
                        self.blocks.insert(index, Block::default());
                    }
                    _ => {}
                }
            }
            Some("content_block_delta") => {
                let d = v.get("delta").cloned().unwrap_or(Value::Null);
                match d.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        let t = d.get("text").and_then(Value::as_str).unwrap_or_default();
                        if let Some(last) = self.texts.last_mut() {
                            last.push_str(t);
                        } else {
                            self.texts.push(t.to_owned());
                        }
                        if !t.is_empty() {
                            out.push(AiDelta::Text { text: t.to_owned() });
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(b) = self.blocks.get_mut(&index) {
                            b.json.push_str(
                                d.get("partial_json")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default(),
                            );
                        }
                    }
                    _ => {}
                }
            }
            Some("content_block_stop") => {
                if let Some(Block {
                    tool: Some((id, name)),
                    json,
                }) = self.blocks.remove(&index)
                {
                    let input = if json.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&json)
                            .map_err(|_| AiError::Answer("a tool call had no valid input".into()))?
                    };
                    let call = AiToolCall { id, name, input };
                    out.push(AiDelta::ToolCall { call: call.clone() });
                    self.calls.push(call);
                }
            }
            Some("message_delta") => {
                if let Some(r) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop = Some(r.to_owned());
                }
                if let Some(n) = v.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                    self.usage.output_tokens = n;
                    self.seen_usage = true;
                }
            }
            Some("error") => {
                let m = v
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("the stream failed");
                return Err(AiError::Provider(m.to_owned()));
            }
            _ => {}
        }
        Ok(())
    }
}

impl Decoder for AnthropicDecoder {
    fn feed(&mut self, chunk: &[u8], out: &mut Vec<AiDelta>) -> Result<(), AiError> {
        let mut events = Vec::new();
        self.sse.feed(chunk, &mut events);
        for e in events {
            self.event(e, out)?;
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>, out: &mut Vec<AiDelta>) -> Result<AiReply, AiError> {
        let mut events = Vec::new();
        self.sse.finish(&mut events);
        for e in events {
            self.event(e, out)?;
        }
        let stop = stop_of(self.stop.as_deref(), !self.calls.is_empty());
        Ok(AiReply {
            text: self.texts.join("\n"),
            tool_calls: self.calls,
            stop,
            usage: self.seen_usage.then_some(self.usage),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider;
    use super::super::testing::*;
    use super::*;
    use crate::ai::tests::block_on;
    use crate::ai::{AiProviderKind, AiRole, AiTool, AiToolResult};

    #[test]
    fn messages_request_and_answer() {
        let t = Fake::new(
            200,
            r#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"answer"}],"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":4}}"#,
        );
        let c = config(AiProviderKind::Anthropic, "claude-opus-5-5", None);
        let p = provider(&t, &c, Some("sk-test".into()), false).unwrap();
        assert_eq!(block_on(p.complete(call())).unwrap(), "answer");
        assert_eq!(
            t.seen.borrow()[0].url,
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(t.header("x-api-key").as_deref(), Some("sk-test"));
        assert_eq!(t.header("anthropic-version").as_deref(), Some("2026-10-21"));
        assert_eq!(t.header("anthropic-beta").as_deref(), Some(FALLBACK_BETA));
        assert_eq!(t.header("anthropic-dangerous-direct-browser-access"), None);
        let body = t.sent_json();
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["system"], "sys");
        assert_eq!(
            body["messages"][0],
            json!({ "role": "user", "content": "hi" })
        );
        assert_eq!(body["thinking"], json!({ "type": "adaptive" }));
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["fallbacks"], "default");
        assert!(body.get("temperature").is_none() && body.get("stream").is_none());
    }

    #[test]
    fn browser_older_model_and_proxy() {
        let t = Fake::new(200, r#"{"content":[{"type":"text","text":"a"}]}"#);
        let c = config(
            AiProviderKind::Anthropic,
            "claude-haiku-4-5",
            Some("https://proxy.example/v1/"),
        );
        let p = provider(&t, &c, Some("k".into()), true).unwrap();
        block_on(p.complete(call())).unwrap();
        assert_eq!(t.seen.borrow()[0].url, "https://proxy.example/v1/messages");
        assert_eq!(
            t.header("anthropic-dangerous-direct-browser-access")
                .as_deref(),
            Some("true")
        );
        assert_eq!(t.header("anthropic-beta"), None);
        let body = t.sent_json();
        assert!(body.get("thinking").is_none() && body.get("fallbacks").is_none());
    }

    #[test]
    fn errors() {
        let c = config(AiProviderKind::Anthropic, "claude-opus-5-5", None);
        assert!(matches!(
            provider(&Fake::new(200, ""), &c, None, false).err(),
            Some(AiError::NoKey)
        ));
        let t = Fake::new(
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key sk-live"}}"#,
        );
        let p = provider(&t, &c, Some("sk-live".into()), false).unwrap();
        assert_eq!(
            block_on(p.complete(call())).unwrap_err(),
            AiError::Provider("401 invalid x-api-key ***".into())
        );
        let t = Fake::new(200, r#"{"content":[],"stop_reason":"refusal"}"#);
        let p = provider(&t, &c, Some("k".into()), false).unwrap();
        assert!(matches!(
            block_on(p.complete(call())),
            Err(AiError::Provider(_))
        ));
        let t = Fake::new(
            200,
            r#"{"content":[{"type":"text","text":"a"}],"stop_reason":"max_tokens"}"#,
        );
        let p = provider(&t, &c, Some("k".into()), false).unwrap();
        assert!(matches!(
            block_on(p.complete(call())),
            Err(AiError::Answer(_))
        ));
    }

    #[test]
    fn tools_json_and_streaming() {
        let t = Fake::new(
            200,
            r#"{"content":[{"type":"text","text":"let me look"},{"type":"tool_use","id":"tu1","name":"get","input":{"p":1}}],"stop_reason":"tool_use","usage":{"input_tokens":9,"output_tokens":3}}"#,
        );
        let c = config(AiProviderKind::Anthropic, "claude-opus-5-5", None);
        let p = provider(&t, &c, Some("k".into()), false).unwrap();
        let mut ask = call();
        ask.tools = vec![AiTool {
            name: "get".into(),
            description: "d".into(),
            input_schema: json!({"type":"object"}),
        }];
        ask.json = Some(json!({"type":"object"}));
        let reply = block_on(p.reply(ask.clone())).unwrap();
        assert_eq!(reply.stop, AiStop::ToolUse);
        assert_eq!(reply.text, "let me look");
        assert_eq!(reply.tool_calls[0].input, json!({"p":1}));
        let b = t.sent_json();
        assert_eq!(b["tools"][0]["input_schema"], json!({"type":"object"}));
        assert_eq!(b["output_config"]["format"]["type"], "json_schema");
        assert_eq!(b["output_config"]["effort"], "medium");
        ask.messages.push(AiMessage {
            role: AiRole::Assistant,
            content: "let me look".into(),
            tool_calls: reply.tool_calls.clone(),
            tool_results: vec![],
        });
        ask.messages.push(AiMessage {
            role: AiRole::User,
            content: String::new(),
            tool_calls: vec![],
            tool_results: vec![AiToolResult {
                call_id: "tu1".into(),
                content: "r".into(),
                is_error: true,
            }],
        });
        block_on(p.reply(ask)).unwrap();
        let sent: Value =
            serde_json::from_slice(&t.seen.borrow()[1].body.as_ref().unwrap().bytes).unwrap();
        assert_eq!(sent["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(
            sent["messages"][2]["content"][0],
            json!({"type":"tool_result","tool_use_id":"tu1","content":"r","is_error":true})
        );

        let sse = [
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":11,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu2\",\"name\":\"get\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"a\\\":\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"5}\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":8}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
        ];
        let t = Fake::chunks(200, &sse, Some("text/event-stream"));
        let p = provider(&t, &c, Some("k".into()), false).unwrap();
        let n = std::cell::RefCell::new(0);
        let reply = block_on(p.stream(call(), &|_| *n.borrow_mut() += 1)).unwrap();
        assert_eq!((reply.text.as_str(), *n.borrow()), ("Hi", 2));
        assert_eq!(
            reply.tool_calls[0],
            AiToolCall {
                id: "tu2".into(),
                name: "get".into(),
                input: json!({"a":5})
            }
        );
        assert_eq!(
            reply.usage,
            Some(AiUsage {
                input_tokens: 11,
                output_tokens: 8
            })
        );
        assert_eq!(t.sent_json()["stream"], true);

        let t = Fake::chunks(
            200,
            &["data: {\"type\":\"error\",\"error\":{\"message\":\"overloaded k\"}}\n\n"],
            Some("text/event-stream"),
        );
        let p = provider(&t, &c, Some("k".into()), false).unwrap();
        assert!(matches!(
            block_on(p.stream(call(), &|_| {})),
            Err(AiError::Provider(_))
        ));
    }
}

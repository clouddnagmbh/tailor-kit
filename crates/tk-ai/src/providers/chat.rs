// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Chat completions (`POST {base}/chat/completions`): OpenAI, Azure OpenAI (v1 API of a
//! resource, `api-key` header, `model` is the deployment), Mistral and any OpenAI-compatible
//! server (Ollama, LM Studio, vLLM, gateways; key optional).
//!
//! Tools are `tools[].function`, calls `tool_calls` with the arguments as a JSON string,
//! results `role: tool` messages. A JSON schema is `response_format: json_schema` (`strict`
//! only when the schema allows it); compatible servers only get it in the prompt. OpenAI and
//! Azure use `max_completion_tokens` (`max_tokens` is refused by newer models). Streaming is
//! SSE chunks ending with `data: [DONE]`.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use tk_net::transport::{HttpRequest, Transport};

use super::sse::{Sse, SseEvent};
use super::{
    Decoder, Wire, Wired, base_url, json_request, role, schema_in_prompt, strict_compatible,
};
use crate::{
    AiCall, AiConfig, AiDelta, AiError, AiMessage, AiProviderKind, AiReply, AiRole, AiStop,
    AiToolCall, AiUsage,
};

const OPENAI_BASE: &str = "https://api.openai.com/v1";
const MISTRAL_BASE: &str = "https://api.mistral.ai/v1";

fn default_base(kind: AiProviderKind) -> &'static str {
    match kind {
        AiProviderKind::Openai => OPENAI_BASE,
        AiProviderKind::Mistral => MISTRAL_BASE,
        _ => "",
    }
}

pub(super) fn url(config: &AiConfig) -> String {
    format!(
        "{}/chat/completions",
        base_url(config, default_base(config.provider))
    )
}

pub struct Chat {
    kind: AiProviderKind,
    key: Option<String>,
    model: String,
    url: String,
}

/// Any chat completions server.
pub type ChatProvider<'t, T> = Wired<'t, T, Chat>;
/// The wave-8 name of [`ChatProvider`].
pub type OpenAiCompatibleProvider<'t, T> = Wired<'t, T, Chat>;

impl<'t, T: Transport> Wired<'t, T, Chat> {
    /// `key` is required except for `openai-compatible` ([`AiError::NoKey`]); a missing base
    /// URL is [`AiError::NotConfigured`] where the kind has no default.
    pub fn new(transport: &'t T, config: &AiConfig, key: Option<String>) -> Result<Self, AiError> {
        config.validate()?;
        let key = key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
        if config.provider.needs_key() && key.is_none() {
            return Err(AiError::NoKey);
        }
        if base_url(config, default_base(config.provider)).is_empty() {
            return Err(AiError::NotConfigured);
        }
        Ok(Self {
            transport,
            wire: Chat {
                kind: config.provider,
                key,
                model: config.model.trim().to_owned(),
                url: url(config),
            },
        })
    }

    /// The HTTP request for `call` (not streaming).
    pub fn request(&self, call: &AiCall) -> HttpRequest {
        self.wire.request(call, false)
    }
}

fn content_of(m: &AiMessage) -> Value {
    if m.content.is_empty() && !m.tool_calls.is_empty() {
        Value::Null
    } else {
        json!(m.content)
    }
}

impl Chat {
    fn messages(&self, call: &AiCall) -> Vec<Value> {
        let system = match (&call.json, self.kind) {
            (Some(schema), AiProviderKind::OpenaiCompatible) => {
                schema_in_prompt(&call.system, schema)
            }
            _ => call.system.clone(),
        };
        let mut out = vec![json!({ "role": "system", "content": system })];
        for m in &call.messages {
            for r in &m.tool_results {
                let content = if r.is_error {
                    format!("Error: {}", r.content)
                } else {
                    r.content.clone()
                };
                out.push(json!({ "role": "tool", "tool_call_id": r.call_id, "content": content }));
            }
            let calls: Vec<Value> = m
                .tool_calls
                .iter()
                .map(|c| json!({ "id": c.id, "type": "function", "function": { "name": c.name, "arguments": c.input.to_string() } }))
                .collect();
            if m.role == AiRole::Assistant && !calls.is_empty() {
                out.push(
                    json!({ "role": "assistant", "content": content_of(m), "tool_calls": calls }),
                );
            } else if !m.content.is_empty() || m.tool_results.is_empty() {
                out.push(json!({ "role": role(m.role), "content": m.content }));
            }
        }
        out
    }
}

impl Wire for Chat {
    fn request(&self, call: &AiCall, stream: bool) -> HttpRequest {
        let tokens = match self.kind {
            AiProviderKind::Openai | AiProviderKind::AzureOpenai => "max_completion_tokens",
            _ => "max_tokens",
        };
        let mut body = json!({
            "model": self.model,
            "messages": self.messages(call),
            "stream": stream,
        });
        body[tokens] = json!(call.max_tokens);
        if stream
            && matches!(
                self.kind,
                AiProviderKind::Openai | AiProviderKind::AzureOpenai
            )
        {
            body["stream_options"] = json!({ "include_usage": true });
        }
        if !call.tools.is_empty() {
            body["tools"] = call
                .tools
                .iter()
                .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.input_schema } }))
                .collect();
        }
        if let (Some(schema), false) = (&call.json, self.kind == AiProviderKind::OpenaiCompatible) {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": { "name": "answer", "schema": schema, "strict": strict_compatible(schema) },
            });
        }
        let mut headers = Vec::new();
        if let Some(key) = &self.key {
            match self.kind {
                AiProviderKind::AzureOpenai => headers.push(("api-key".to_owned(), key.clone())),
                _ => headers.push(("authorization".to_owned(), format!("Bearer {key}"))),
            }
        }
        json_request(self.url.clone(), headers, &body)
    }

    fn key(&self) -> &str {
        self.key.as_deref().unwrap_or_default()
    }

    fn parse(&self, body: &Value) -> Result<AiReply, AiError> {
        let choice = body.pointer("/choices/0");
        let message = choice.and_then(|c| c.get("message"));
        let text = message
            .and_then(|m| m.get("content"))
            .map(text_of)
            .unwrap_or_default();
        let mut tool_calls = Vec::new();
        for c in message
            .and_then(|m| m.get("tool_calls"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            tool_calls.push(call_of(
                c.get("id").and_then(Value::as_str).unwrap_or_default(),
                c.pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                c.pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )?);
        }
        let refusal = message
            .and_then(|m| m.get("refusal"))
            .and_then(Value::as_str)
            .is_some_and(|r| !r.is_empty());
        let stop = stop_of(
            choice
                .and_then(|c| c.get("finish_reason"))
                .and_then(Value::as_str),
            !tool_calls.is_empty(),
            refusal,
        );
        Ok(AiReply {
            text,
            tool_calls,
            stop,
            usage: usage_of(body.get("usage")),
        })
    }

    fn decoder(&self) -> Box<dyn Decoder> {
        Box::new(ChatDecoder::default())
    }

    fn tools_refused(&self, status: u16) -> bool {
        self.kind == AiProviderKind::OpenaiCompatible && matches!(status, 400 | 404 | 422 | 501)
    }
}

/// Content as a string, or the text parts of an array (Mistral).
fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn call_of(id: &str, name: &str, arguments: &str) -> Result<AiToolCall, AiError> {
    let input = if arguments.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(arguments)
            .map_err(|_| AiError::Answer("a tool call had no valid input".into()))?
    };
    Ok(AiToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        input,
    })
}

fn stop_of(reason: Option<&str>, tools: bool, refusal: bool) -> AiStop {
    match reason {
        Some("length" | "model_length") => AiStop::MaxTokens,
        Some("tool_calls") => AiStop::ToolUse,
        Some("content_filter") => AiStop::Refusal,
        _ if refusal => AiStop::Refusal,
        _ if tools => AiStop::ToolUse,
        _ => AiStop::End,
    }
}

fn usage_of(u: Option<&Value>) -> Option<AiUsage> {
    let u = u.filter(|u| u.is_object())?;
    Some(AiUsage {
        input_tokens: u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
        output_tokens: u
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

#[derive(Default)]
struct Partial {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct ChatDecoder {
    sse: Sse,
    text: String,
    calls: BTreeMap<u64, Partial>,
    finish: Option<String>,
    refusal: bool,
    usage: Option<AiUsage>,
}

impl ChatDecoder {
    fn event(&mut self, e: SseEvent, out: &mut Vec<AiDelta>) -> Result<(), AiError> {
        if e.data.trim() == "[DONE]" {
            return Ok(());
        }
        let Ok(v) = serde_json::from_str::<Value>(&e.data) else {
            return Ok(());
        };
        if let Some(err) = v.get("error") {
            let m = err
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| err.as_str())
                .unwrap_or("the stream failed");
            return Err(AiError::Provider(m.to_owned()));
        }
        if let Some(u) = usage_of(v.get("usage")) {
            self.usage = Some(u);
        }
        let Some(choice) = v.pointer("/choices/0") else {
            return Ok(());
        };
        if let Some(r) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish = Some(r.to_owned());
        }
        let delta = choice.get("delta").unwrap_or(&Value::Null);
        let t = delta.get("content").map(text_of).unwrap_or_default();
        if !t.is_empty() {
            self.text.push_str(&t);
            out.push(AiDelta::Text { text: t });
        }
        if delta
            .get("refusal")
            .and_then(Value::as_str)
            .is_some_and(|r| !r.is_empty())
        {
            self.refusal = true;
        }
        for c in delta
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let index = c.get("index").and_then(Value::as_u64).unwrap_or(0);
            let p = self.calls.entry(index).or_default();
            if let Some(id) = c.get("id").and_then(Value::as_str) {
                p.id = id.to_owned();
            }
            if let Some(n) = c.pointer("/function/name").and_then(Value::as_str) {
                p.name.push_str(n);
            }
            if let Some(a) = c.pointer("/function/arguments").and_then(Value::as_str) {
                p.arguments.push_str(a);
            }
        }
        Ok(())
    }
}

impl Decoder for ChatDecoder {
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
        let mut tool_calls = Vec::new();
        for p in std::mem::take(&mut self.calls).into_values() {
            let call = call_of(&p.id, &p.name, &p.arguments)?;
            out.push(AiDelta::ToolCall { call: call.clone() });
            tool_calls.push(call);
        }
        let stop = stop_of(self.finish.as_deref(), !tool_calls.is_empty(), self.refusal);
        Ok(AiReply {
            text: self.text,
            tool_calls,
            stop,
            usage: self.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider;
    use super::super::testing::*;
    use super::*;
    use crate::block_on;
    use crate::{AiTool, AiToolResult};

    #[test]
    fn compatible_with_and_without_a_key() {
        let t = Fake::new(
            200,
            r#"{"choices":[{"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}]}"#,
        );
        let c = config(
            AiProviderKind::OpenaiCompatible,
            "llama3.1",
            Some("http://localhost:11434/v1/"),
        );
        let p = provider(&t, &c, None, true).unwrap();
        assert_eq!(block_on(p.complete(call())).unwrap(), "hello");
        assert_eq!(
            t.seen.borrow()[0].url,
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(t.header("authorization"), None);
        let body = t.sent_json();
        assert_eq!(
            body["messages"][0],
            json!({ "role": "system", "content": "sys" })
        );
        assert_eq!(
            body["messages"][1],
            json!({ "role": "user", "content": "hi" })
        );
        assert_eq!(body["max_tokens"], 100);

        let t = Fake::new(500, r#"{"error":"model not found tok-1"}"#);
        let p = provider(&t, &c, Some("tok-1".into()), false).unwrap();
        assert_eq!(
            block_on(p.complete(call())).unwrap_err(),
            AiError::Provider("500 model not found ***".into())
        );
        assert_eq!(t.header("authorization").as_deref(), Some("Bearer tok-1"));

        let no_base = config(AiProviderKind::OpenaiCompatible, "m", None);
        assert!(matches!(
            provider(&t, &no_base, None, false).err(),
            Some(AiError::NotConfigured)
        ));
    }

    #[test]
    fn headers_and_token_fields_per_kind() {
        let t = Fake::new(200, r#"{"choices":[{"message":{"content":"x"}}]}"#);
        let az = config(
            AiProviderKind::AzureOpenai,
            "my-deployment",
            Some("https://r.openai.azure.com/openai/v1"),
        );
        block_on(
            provider(&t, &az, Some("azk".into()), true)
                .unwrap()
                .complete(call()),
        )
        .unwrap();
        assert_eq!(
            t.seen.borrow()[0].url,
            "https://r.openai.azure.com/openai/v1/chat/completions"
        );
        assert_eq!(t.header("api-key").as_deref(), Some("azk"));
        assert_eq!(t.header("authorization"), None);
        assert!(t.sent_json().get("max_completion_tokens").is_some());

        let t = Fake::new(
            200,
            r#"{"choices":[{"message":{"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}}]}"#,
        );
        let m = config(AiProviderKind::Mistral, "mistral-large-latest", None);
        assert_eq!(
            block_on(
                provider(&t, &m, Some("mk".into()), false)
                    .unwrap()
                    .complete(call())
            )
            .unwrap(),
            "ab"
        );
        assert_eq!(
            t.seen.borrow()[0].url,
            "https://api.mistral.ai/v1/chat/completions"
        );
        assert_eq!(t.sent_json()["max_tokens"], 100);

        let o = config(AiProviderKind::Openai, "gpt", None);
        assert!(matches!(
            provider(&t, &o, None, false).err(),
            Some(AiError::NoKey)
        ));
    }

    #[test]
    fn tool_round_trip_and_json_schema() {
        let t = Fake::new(
            200,
            r#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"get","arguments":"{\"p\":1}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}"#,
        );
        let o = config(AiProviderKind::Openai, "gpt", None);
        let p = provider(&t, &o, Some("k".into()), false).unwrap();
        let mut c = call();
        c.tools = vec![AiTool {
            name: "get".into(),
            description: "d".into(),
            input_schema: json!({"type":"object"}),
        }];
        c.json = Some(
            json!({"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"additionalProperties":false}),
        );
        let reply = block_on(p.reply(c.clone())).unwrap();
        assert_eq!(reply.stop, AiStop::ToolUse);
        assert_eq!(
            reply.tool_calls[0],
            AiToolCall {
                id: "c1".into(),
                name: "get".into(),
                input: json!({"p":1})
            }
        );
        assert_eq!(
            reply.usage,
            Some(AiUsage {
                input_tokens: 5,
                output_tokens: 2
            })
        );
        let body = t.sent_json();
        assert_eq!(body["tools"][0]["function"]["name"], "get");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        c.messages.push(AiMessage {
            role: AiRole::Assistant,
            content: String::new(),
            tool_calls: reply.tool_calls.clone(),
            tool_results: vec![],
        });
        c.messages.push(AiMessage {
            role: AiRole::User,
            content: String::new(),
            tool_calls: vec![],
            tool_results: vec![AiToolResult {
                call_id: "c1".into(),
                content: "ok".into(),
                is_error: false,
            }],
        });
        block_on(p.reply(c)).unwrap();
        let sent: Value =
            serde_json::from_slice(&t.seen.borrow()[1].body.as_ref().unwrap().bytes).unwrap();
        let m = sent["messages"].as_array().unwrap();
        assert_eq!(m[2]["tool_calls"][0]["function"]["arguments"], "{\"p\":1}");
        assert_eq!(
            m[3],
            json!({"role":"tool","tool_call_id":"c1","content":"ok"})
        );
    }

    #[test]
    fn streaming_text_tools_and_usage() {
        let sse = [
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"He\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"llo\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c9\",\"function\":{\"name\":\"get\",\"arguments\":\"{\\\"a\\\"\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":2}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n",
        ];
        let t = Fake::chunks(200, &sse, Some("text/event-stream"));
        let o = config(AiProviderKind::Openai, "gpt", None);
        let p = provider(&t, &o, Some("k".into()), false).unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        let reply = block_on(p.stream(call(), &|d| seen.borrow_mut().push(d))).unwrap();
        assert_eq!(reply.text, "Hello");
        assert_eq!(reply.stop, AiStop::ToolUse);
        assert_eq!(reply.tool_calls[0].input, json!({"a":2}));
        assert_eq!(
            reply.usage,
            Some(AiUsage {
                input_tokens: 7,
                output_tokens: 3
            })
        );
        assert_eq!(seen.borrow().len(), 3);
        let body = t.sent_json();
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn ignored_stream_flag_and_refused_tools() {
        let t = Fake::chunks(
            200,
            &[r#"{"choices":[{"message":{"content":"whole"}}]}"#],
            Some("application/json"),
        );
        let c = config(
            AiProviderKind::OpenaiCompatible,
            "m",
            Some("http://localhost:1/v1"),
        );
        let p = provider(&t, &c, None, false).unwrap();
        let reply = block_on(p.stream(call(), &|_| {})).unwrap();
        assert_eq!(reply.text, "whole");

        let t = Fake::new(400, r#"{"error":"x does not support tools"}"#);
        let p = provider(&t, &c, None, false).unwrap();
        let mut with_tools = call();
        with_tools.tools = vec![AiTool {
            name: "x".into(),
            description: String::new(),
            input_schema: json!({}),
        }];
        assert!(matches!(
            block_on(p.reply(with_tools)),
            Err(AiError::Unsupported(_))
        ));
        let mut j = call();
        j.json = Some(json!({"type":"object"}));
        let _ = block_on(p.reply(j));
        let sent: Value =
            serde_json::from_slice(&t.seen.borrow()[1].body.as_ref().unwrap().bytes).unwrap();
        assert!(
            sent["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("JSON Schema")
        );
        assert!(sent.get("response_format").is_none());
    }
}

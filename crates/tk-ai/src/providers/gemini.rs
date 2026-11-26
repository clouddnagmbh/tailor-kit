// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Google Gemini (`POST /v1beta/models/<model>:generateContent`, streaming with
//! `:streamGenerateContent?alt=sse`, header `x-goog-api-key`).
//!
//! - System prompt: `systemInstruction`; roles `user` and `model`; limit `maxOutputTokens`.
//! - Tools: `functionDeclarations`; calls are `functionCall` parts, results `functionResponse`
//!   parts in a user message (they name the function, so the call's name is looked up by id).
//!   A model with thinking may attach a `thoughtSignature` to a call that must come back with
//!   it; it travels inside [`AiToolCall::id`] after a `~` and is split off again here.
//! - JSON: `responseMimeType: application/json` and `responseJsonSchema`. Combined with
//!   tools it is only valid on Gemini 3 and later, so with tools the schema goes into the
//!   prompt instead.

use serde_json::{Value, json};
use tk_net::transport::{HttpRequest, Transport};

use super::sse::{Sse, SseEvent};
use super::{Decoder, Wire, Wired, base_url, json_request, schema_in_prompt};
use crate::{AiCall, AiConfig, AiDelta, AiError, AiReply, AiRole, AiStop, AiToolCall, AiUsage};

/// Google's public endpoint.
pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";

fn model_path(model: &str) -> String {
    let m = model.trim();
    format!("models/{}", m.strip_prefix("models/").unwrap_or(m))
}

pub(super) fn url(config: &AiConfig, stream: bool) -> String {
    let base = base_url(config, GEMINI_BASE_URL);
    let model = model_path(&config.model);
    if stream {
        format!("{base}/v1beta/{model}:streamGenerateContent?alt=sse")
    } else {
        format!("{base}/v1beta/{model}:generateContent")
    }
}

pub struct Gemini {
    key: String,
    config: AiConfig,
}

/// The Gemini API.
pub type GeminiProvider<'t, T> = Wired<'t, T, Gemini>;

impl<'t, T: Transport> Wired<'t, T, Gemini> {
    /// `key` is required ([`AiError::NoKey`]).
    pub fn new(transport: &'t T, config: &AiConfig, key: Option<String>) -> Result<Self, AiError> {
        let key = key.filter(|k| !k.trim().is_empty()).ok_or(AiError::NoKey)?;
        Ok(Self {
            transport,
            wire: Gemini {
                key: key.trim().to_owned(),
                config: config.clone(),
            },
        })
    }
}

/// `(id, signature)` of a tool-call id made by this module.
fn split_id(id: &str) -> (&str, Option<&str>) {
    match id.split_once('~') {
        Some((i, s)) => (i, Some(s)),
        None => (id, None),
    }
}

impl Wire for Gemini {
    fn request(&self, call: &AiCall, stream: bool) -> HttpRequest {
        let schema_in_mode = call.json.is_some() && call.tools.is_empty();
        let system = match (&call.json, schema_in_mode) {
            (Some(schema), false) => schema_in_prompt(&call.system, schema),
            _ => call.system.clone(),
        };
        // Function names by call id, for the results.
        let names: Vec<(&str, &str)> = call
            .messages
            .iter()
            .flat_map(|m| &m.tool_calls)
            .map(|c| (c.id.as_str(), c.name.as_str()))
            .collect();
        let mut contents = Vec::new();
        for m in &call.messages {
            let mut parts = Vec::new();
            for r in &m.tool_results {
                let name = names
                    .iter()
                    .find(|(i, _)| *i == r.call_id)
                    .map_or("tool", |(_, n)| n);
                let (id, _) = split_id(&r.call_id);
                let response = if r.is_error {
                    json!({ "error": r.content })
                } else {
                    json!({ "output": r.content })
                };
                let mut fr = json!({ "name": name, "response": response });
                if !id.starts_with('g') || id.len() > 3 {
                    fr["id"] = json!(id);
                }
                parts.push(json!({ "functionResponse": fr }));
            }
            if !m.content.is_empty() {
                parts.push(json!({ "text": m.content }));
            }
            for c in &m.tool_calls {
                let (id, sig) = split_id(&c.id);
                let mut fc = json!({ "name": c.name, "args": c.input });
                if !(id.starts_with('g') && id.len() <= 3) {
                    fc["id"] = json!(id);
                }
                let mut part = json!({ "functionCall": fc });
                if let Some(s) = sig {
                    part["thoughtSignature"] = json!(s);
                }
                parts.push(part);
            }
            if parts.is_empty() {
                parts.push(json!({ "text": "" }));
            }
            contents.push(json!({
                "role": if m.role == AiRole::Assistant { "model" } else { "user" },
                "parts": parts,
            }));
        }
        let mut generation = json!({ "maxOutputTokens": call.max_tokens });
        if let (Some(schema), true) = (&call.json, schema_in_mode) {
            generation["responseMimeType"] = json!("application/json");
            generation["responseJsonSchema"] = schema.clone();
        }
        let mut body = json!({
            "systemInstruction": { "parts": [{ "text": system }] },
            "contents": contents,
            "generationConfig": generation,
        });
        if !call.tools.is_empty() {
            body["tools"] = json!([{ "functionDeclarations": call.tools.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "parametersJsonSchema": t.input_schema,
            })).collect::<Vec<_>>() }]);
        }
        let mut req = json_request(
            url(&self.config, stream),
            vec![("x-goog-api-key".to_owned(), self.key.clone())],
            &body,
        );
        if stream {
            req.headers.retain(|(n, _)| n != "accept");
            req.headers
                .push(("accept".into(), "text/event-stream".into()));
        }
        req
    }

    fn key(&self) -> &str {
        &self.key
    }

    fn parse(&self, body: &Value) -> Result<AiReply, AiError> {
        let mut d = GeminiDecoder::default();
        d.chunk(body, &mut Vec::new())?;
        d.reply(&mut Vec::new())
    }

    fn decoder(&self) -> Box<dyn Decoder> {
        Box::new(GeminiDecoder::default())
    }
}

#[derive(Default)]
struct GeminiDecoder {
    sse: Sse,
    text: String,
    calls: Vec<AiToolCall>,
    finish: Option<String>,
    blocked: bool,
    usage: Option<AiUsage>,
}

impl GeminiDecoder {
    /// One `GenerateContentResponse` (a whole answer or a streamed piece).
    fn chunk(&mut self, v: &Value, out: &mut Vec<AiDelta>) -> Result<(), AiError> {
        if let Some(err) = v.get("error") {
            let m = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the stream failed");
            return Err(AiError::Provider(m.to_owned()));
        }
        if v.pointer("/promptFeedback/blockReason").is_some() {
            self.blocked = true;
        }
        if let Some(u) = v.get("usageMetadata") {
            let n = |k| u.get(k).and_then(Value::as_u64).unwrap_or(0);
            self.usage = Some(AiUsage {
                input_tokens: n("promptTokenCount"),
                output_tokens: n("candidatesTokenCount") + n("thoughtsTokenCount"),
            });
        }
        let Some(c) = v.pointer("/candidates/0") else {
            return Ok(());
        };
        if let Some(r) = c.get("finishReason").and_then(Value::as_str) {
            self.finish = Some(r.to_owned());
        }
        for part in c
            .pointer("/content/parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(fc) = part.get("functionCall") {
                let n = self.calls.len();
                let id = fc
                    .get("id")
                    .and_then(Value::as_str)
                    .map_or_else(|| format!("g{n}"), str::to_owned);
                let id = match part.get("thoughtSignature").and_then(Value::as_str) {
                    Some(sig) => format!("{id}~{sig}"),
                    None => id,
                };
                let call = AiToolCall {
                    id,
                    name: fc
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    input: fc.get("args").cloned().unwrap_or_else(|| json!({})),
                };
                out.push(AiDelta::ToolCall { call: call.clone() });
                self.calls.push(call);
            } else if let Some(t) = part.get("text").and_then(Value::as_str) {
                if part.get("thought").and_then(Value::as_bool) == Some(true) || t.is_empty() {
                    continue;
                }
                self.text.push_str(t);
                out.push(AiDelta::Text { text: t.to_owned() });
            }
        }
        Ok(())
    }

    fn reply(&mut self, _: &mut Vec<AiDelta>) -> Result<AiReply, AiError> {
        let stop = match self.finish.as_deref() {
            Some("MAX_TOKENS") => AiStop::MaxTokens,
            Some(
                "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII"
                | "IMAGE_SAFETY",
            ) => AiStop::Refusal,
            _ if self.blocked && self.text.is_empty() => AiStop::Refusal,
            _ if !self.calls.is_empty() => AiStop::ToolUse,
            _ => AiStop::End,
        };
        Ok(AiReply {
            text: std::mem::take(&mut self.text),
            tool_calls: std::mem::take(&mut self.calls),
            stop,
            usage: self.usage,
        })
    }

    fn event(&mut self, e: SseEvent, out: &mut Vec<AiDelta>) -> Result<(), AiError> {
        match serde_json::from_str::<Value>(&e.data) {
            Ok(v) => self.chunk(&v, out),
            Err(_) => Ok(()),
        }
    }
}

impl Decoder for GeminiDecoder {
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
        self.reply(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::provider;
    use super::super::testing::*;
    use super::*;
    use crate::block_on;
    use crate::{AiMessage, AiProviderKind, AiTool, AiToolResult};

    fn cfg() -> AiConfig {
        config(AiProviderKind::Gemini, "gemini-3-pro", None)
    }

    #[test]
    fn request_answer_and_json_mode() {
        let t = Fake::new(
            200,
            r#"{"candidates":[{"content":{"parts":[{"text":"a"},{"text":"b"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":4,"candidatesTokenCount":2}}"#,
        );
        let p = provider(&t, &cfg(), Some("gk".into()), true).unwrap();
        let mut c = call();
        c.json = Some(json!({"type":"object"}));
        let reply = block_on(p.reply(c)).unwrap();
        assert_eq!(reply.text, "ab");
        assert_eq!(
            reply.usage,
            Some(AiUsage {
                input_tokens: 4,
                output_tokens: 2
            })
        );
        assert_eq!(
            t.seen.borrow()[0].url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro:generateContent"
        );
        assert_eq!(t.header("x-goog-api-key").as_deref(), Some("gk"));
        let b = t.sent_json();
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(
            b["contents"][0],
            json!({"role":"user","parts":[{"text":"hi"}]})
        );
        assert_eq!(
            b["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(b["generationConfig"]["maxOutputTokens"], 100);
        assert!(matches!(
            provider(&t, &cfg(), None, false).err(),
            Some(AiError::NoKey)
        ));
    }

    #[test]
    fn tools_round_trip_keep_the_signature_and_skip_json_mode() {
        let t = Fake::new(
            200,
            r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get","args":{"p":1}},"thoughtSignature":"SIG"}]},"finishReason":"STOP"}]}"#,
        );
        let p = provider(&t, &cfg(), Some("gk".into()), false).unwrap();
        let mut c = call();
        c.tools = vec![AiTool {
            name: "get".into(),
            description: "d".into(),
            input_schema: json!({"type":"object"}),
        }];
        c.json = Some(json!({"type":"object"}));
        let reply = block_on(p.reply(c.clone())).unwrap();
        assert_eq!(reply.stop, AiStop::ToolUse);
        assert_eq!(reply.tool_calls[0].id, "g0~SIG");
        let b = t.sent_json();
        assert_eq!(
            b["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"],
            json!({"type":"object"})
        );
        assert!(b["generationConfig"].get("responseMimeType").is_none());
        assert!(
            b["systemInstruction"]["parts"][0]["text"]
                .as_str()
                .unwrap()
                .contains("JSON Schema")
        );
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
                call_id: "g0~SIG".into(),
                content: "ok".into(),
                is_error: false,
            }],
        });
        block_on(p.reply(c)).unwrap();
        let sent: Value =
            serde_json::from_slice(&t.seen.borrow()[1].body.as_ref().unwrap().bytes).unwrap();
        assert_eq!(sent["contents"][1]["parts"][0]["thoughtSignature"], "SIG");
        assert_eq!(
            sent["contents"][1]["parts"][0]["functionCall"]["name"],
            "get"
        );
        assert_eq!(
            sent["contents"][2]["parts"][0]["functionResponse"]["name"],
            "get"
        );
    }

    #[test]
    fn streaming_and_errors() {
        let sse = [
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\r\n\r\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":2}}\r\n\r\n",
        ];
        let t = Fake::chunks(200, &sse, Some("text/event-stream"));
        let p = provider(&t, &cfg(), Some("gk".into()), false).unwrap();
        let seen = std::cell::RefCell::new(0);
        let reply = block_on(p.stream(call(), &|_| *seen.borrow_mut() += 1)).unwrap();
        assert_eq!((reply.text.as_str(), *seen.borrow()), ("Hello", 2));
        assert!(
            t.seen.borrow()[0]
                .url
                .ends_with(":streamGenerateContent?alt=sse")
        );

        let t = Fake::new(
            400,
            r#"{"error":{"code":400,"message":"API key not valid gk","status":"INVALID_ARGUMENT"}}"#,
        );
        let p = provider(&t, &cfg(), Some("gk".into()), false).unwrap();
        assert_eq!(
            block_on(p.complete(call())).unwrap_err(),
            AiError::Provider("400 API key not valid ***".into())
        );
        let t = Fake::new(200, r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#);
        let p = provider(&t, &cfg(), Some("gk".into()), false).unwrap();
        assert!(matches!(
            block_on(p.complete(call())),
            Err(AiError::Provider(_))
        ));
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Wave 16 interface: provider-neutral tool use, streaming, and the generic call that
//! extensions make through the configured provider.
//!
//! - Tools ([`AiTool`]): a name, a description and a JSON Schema of the input. The model
//!   answers with [`AiToolCall`]s; the caller runs them (read the collection, propose an edit,
//!   send only with the user's approval) and continues with [`AiToolResult`]s. Each provider
//!   maps this to its own shape (Anthropic `tool_use`/`tool_result`, OpenAI `tool_calls` and
//!   `role: tool`, Gemini `functionCall`/`functionResponse`, Bedrock Converse `toolUse`).
//! - [`AiRequest`]: what an extension asks for (`ExtensionContext.ai` in the UI). The core
//!   adds the masked context of [`AiScope`] and masks the whole call with every secret value
//!   in scope ([`mask_call`]) before anything leaves, like the wave-8 tasks.
//! - [`AiPreview`]: the exact, masked call that will be sent ("show what will be sent").
//! - [`AiEvent`]: what a streaming call reports while it runs.
//!
//! Owner: wave 16 interface. Implementation: strand A (providers, shells), strand C (tool loop
//! of the assistant panel).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{AiCall, AiError, AiMessage, AiProviderKind, redact};

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiTool {
    /// `[a-zA-Z0-9_-]`, at most 64 characters (the common limit of all providers).
    pub name: String,
    pub description: String,
    /// JSON Schema (draft 2020-12 subset: `type`, `properties`, `required`, `items`, `enum`,
    /// `description`) of the input object.
    pub input_schema: Value,
}

/// A call of a tool in the model's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiToolCall {
    /// The provider's id of the call; the result names it.
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// The result of a tool call, sent back in the next user message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiToolResult {
    pub call_id: String,
    /// Text or JSON as text; masked like every other part of the call.
    pub content: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiStop {
    #[default]
    End,
    /// The answer hit `max_tokens`.
    MaxTokens,
    /// The model wants tool results ([`AiReply::tool_calls`]).
    ToolUse,
    /// The model declined.
    Refusal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// A model's answer to one call.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiReply {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<AiToolCall>,
    #[serde(default)]
    pub stop: AiStop,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AiUsage>,
}

/// A piece of a streaming answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AiDelta {
    Text { text: String },
    ToolCall { call: AiToolCall },
}

/// What a call reports to the UI while it runs (desktop: a Tauri channel; web: a callback).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AiEvent {
    Delta { delta: AiDelta },
    Done { reply: AiReply },
    Error { error: AiError },
}

/// What of the active collection the core puts into the call, masked (an [`super::AiContext`]).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiScope {
    /// A request path: its spec as written, variables by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// A response of the response store: its masked report, body cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<u64>,
    /// A folder path (`""`: the root): its requests in short form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// A call an extension asks for: its own prompt, the core's masked context, optional tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiRequest {
    /// Chosen by the caller; `AiApi.cancel(id)` stops the call.
    pub id: String,
    /// One line for the preview and the audit: what the call is for ("Write a tests script").
    pub purpose: String,
    /// A role (`fast`, `smart`) or a profile id; `None`: the default profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub system: String,
    pub messages: Vec<AiMessage>,
    #[serde(default)]
    pub scope: AiScope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<AiTool>,
    /// JSON Schema the answer must follow (structured output where the provider has it,
    /// else asked for in the prompt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<Value>,
    /// `None`: the core's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// Exactly what a call will send, already masked. Nothing has left yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiPreview {
    pub profile: String,
    pub provider: AiProviderKind,
    pub model: String,
    /// Scheme, host and path of the endpoint; never a key.
    pub endpoint: String,
    pub call: AiCall,
    /// How many secret occurrences masking replaced.
    pub masked: u32,
    /// Size of the serialized call.
    pub bytes: u64,
    /// The settings ask for this preview to be confirmed before the call
    /// (`AiSettings::must_preview`).
    pub required: bool,
}

/// `call` with every string masked with `secrets`, then the whole serialized payload checked
/// again: what [`super::prepare`] does for the wave-8 tasks, for any call. If the serialized
/// form still contained a secret (an escape the string pass did not see) and the masked text
/// is no longer JSON, nothing may be sent.
pub fn mask_call(call: AiCall, secrets: &[String]) -> Result<AiCall, AiError> {
    let m = |s: &str| redact(s, secrets);
    let call = AiCall {
        system: m(&call.system),
        messages: call
            .messages
            .into_iter()
            .map(|msg| AiMessage {
                role: msg.role,
                content: m(&msg.content),
                tool_calls: msg
                    .tool_calls
                    .into_iter()
                    .map(|c| AiToolCall {
                        id: c.id,
                        name: c.name,
                        input: mask_value(c.input, secrets),
                    })
                    .collect(),
                tool_results: msg
                    .tool_results
                    .into_iter()
                    .map(|r| AiToolResult {
                        call_id: r.call_id,
                        content: m(&r.content),
                        is_error: r.is_error,
                    })
                    .collect(),
            })
            .collect(),
        max_tokens: call.max_tokens,
        tools: call
            .tools
            .into_iter()
            .map(|t| AiTool {
                name: t.name,
                description: m(&t.description),
                input_schema: mask_value(t.input_schema, secrets),
            })
            .collect(),
        json: call.json.map(|v| mask_value(v, secrets)),
    };
    let json = serde_json::to_string(&call).map_err(|e| AiError::Transport(e.to_string()))?;
    let masked = redact(&json, secrets);
    if masked == json {
        return Ok(call);
    }
    serde_json::from_str(&masked).map_err(|_| {
        AiError::Transport("the request to the model could not be masked; nothing was sent".into())
    })
}

/// Every string in `value` (keys too) masked.
fn mask_value(value: Value, secrets: &[String]) -> Value {
    match value {
        Value::String(s) => Value::String(redact(&s, secrets)),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(|v| mask_value(v, secrets)).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (redact(&k, secrets), mask_value(v, secrets)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::AiRole;
    use super::*;

    #[test]
    fn tool_calls_results_and_schemas_are_masked() {
        let call = AiCall {
            system: "sys".into(),
            messages: vec![
                AiMessage {
                    role: AiRole::Assistant,
                    content: String::new(),
                    tool_calls: vec![AiToolCall {
                        id: "t1".into(),
                        name: "get_request".into(),
                        input: json!({"path": "a.yml", "s3cr3t-value": ["s3cr3t-value"]}),
                    }],
                    tool_results: Vec::new(),
                },
                AiMessage {
                    role: AiRole::User,
                    content: String::new(),
                    tool_calls: Vec::new(),
                    tool_results: vec![AiToolResult {
                        call_id: "t1".into(),
                        content: "token=s3cr3t-value".into(),
                        is_error: false,
                    }],
                },
            ],
            max_tokens: 10,
            tools: vec![AiTool {
                name: "get_request".into(),
                description: "reads s3cr3t-value".into(),
                input_schema: json!({"type": "object"}),
            }],
            json: None,
        };
        let masked = mask_call(call, &["s3cr3t-value".into()]).unwrap();
        let text = serde_json::to_string(&masked).unwrap();
        assert!(!text.contains("s3cr3t"), "{text}");
        assert_eq!(masked.messages[0].tool_calls[0].id, "t1");
    }

    #[test]
    fn events_serialize_with_a_kind() {
        let e = AiEvent::Delta {
            delta: AiDelta::Text { text: "hi".into() },
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"kind": "delta", "delta": {"kind": "text", "text": "hi"}})
        );
    }
}

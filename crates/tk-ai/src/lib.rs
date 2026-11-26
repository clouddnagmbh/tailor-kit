// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The AI layer shared by RequestTailor and GitTailor (seam N5). Provenance: extracted from
//! RequestTailor's `rt-app` `ai` module (waves 8 and 16).
//!
//! Off until the user configures a provider; no provider is built in or preferred. The kit
//! knows only masked text and tools, never a product's model:
//!
//! - Providers ([`AiProvider`], [`provider`]) over `tk_net::transport::Transport`:
//!   `anthropic`, the chat-completions family (`openai`, `azure-openai`, `mistral`,
//!   `openai-compatible`) and `gemini`. The provider matrix is data
//!   ([`AiProviderKind::capabilities`]).
//! - Profiles and settings ([`AiSettings`], [`AiProfile`]); keys live in the product's secret
//!   store under [`key_slot`] / [`profile_key_slot`] (a [`tk_secrets::SecretSlot`]).
//! - The call ([`AiCall`], [`AiRequest`], [`AiTool`], [`AiReply`], [`AiDelta`]) and its masking
//!   ([`redact`], [`mask_call`], [`mask_error`]): every string of an outgoing call and the
//!   whole serialized payload are masked with every secret value in scope.
//! - The runtime ([`runtime`]): profile resolution, preview ("show what will be sent"),
//!   streaming, cancellation.
//! - The prompt loader ([`prompts`]): versioned prompt files with `<<name>>` values and
//!   `<<include:id>>` fragments; the product supplies the files.
//! - The seam [`AiContextSource`]: what a product implements to hand the kit its already-masked
//!   context, its secret values and its tools for a call.
//!
//! No product, collection or repository knowledge lives here.

use serde::{Deserialize, Serialize};
use tk_net::LocalBoxFuture;
use tk_secrets::SecretSlot;

mod call;
mod context;
pub mod mask;
mod profiles;
pub mod prompts;
mod providers;
pub mod runtime;

pub use call::{
    AiDelta, AiEvent, AiPreview, AiReply, AiRequest, AiStop, AiTool, AiToolCall, AiToolResult,
    AiUsage, mask_call,
};
pub use context::{AiContextSource, ContextBlock};
pub use profiles::{
    AiCapabilities, AiJsonMode, AiPreviewMode, AiProfile, AiRoles, AiSettings, LEGACY_PROFILE,
    ROLE_FAST, ROLE_SMART, profile_key_slot,
};
pub use providers::{
    ANTHROPIC_BASE_URL, ANTHROPIC_VERSION, AnthropicProvider, ChatProvider, GEMINI_BASE_URL,
    GeminiProvider, OpenAiCompatibleProvider, endpoint_of, provider, reply_text, request_url,
};
pub use runtime::{
    CallGuard, Calls, CancelToken, DEFAULT_MAX_TOKENS, Prepared, ask_text, execute,
    prepare_request, preview, request_call, resolve,
};

/// The model suggested when the user picks `anthropic` (they can change it).
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-opus-5-5";
/// [`SecretSlot::group`] of the provider keys.
pub const KEY_GROUP: &str = "keys";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiProviderKind {
    Anthropic,
    OpenaiCompatible,
    /// Wave 16: OpenAI's own API (`https://api.openai.com/v1`).
    Openai,
    /// Wave 16: Azure OpenAI, v1 API of a resource (`base_url`
    /// `https://<resource>.openai.azure.com/openai/v1`; `model` is the deployment name).
    AzureOpenai,
    /// Wave 16: Google Gemini API (`https://generativelanguage.googleapis.com`).
    Gemini,
    /// Wave 16: Mistral (`https://api.mistral.ai/v1`).
    Mistral,
    /// Wave 16, optional: AWS Bedrock Converse API (`base_url`
    /// `https://bedrock-runtime.<region>.amazonaws.com`; key: a Bedrock API key).
    Bedrock,
}

/// The user's configuration, stored in the app's settings (not in a project).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiConfig {
    pub provider: AiProviderKind,
    /// e.g. `claude-opus-5-5` for Anthropic, `llama3.1` for a local Ollama.
    pub model: String,
    /// `None`: the provider's public endpoint. For `openai-compatible` required, e.g.
    /// `http://localhost:11434/v1`.
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
    /// Wave 16, assistant messages: the tools the model called.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<AiToolCall>,
    /// Wave 16, user messages: the results of those calls.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<AiToolResult>,
}

impl AiMessage {
    /// A plain text message.
    pub fn text(role: AiRole, content: impl Into<String>) -> Self {
        AiMessage {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
        }
    }
}

/// One model call, provider-neutral. Already masked when it reaches a provider.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiCall {
    pub system: String,
    pub messages: Vec<AiMessage>,
    pub max_tokens: u32,
    /// Wave 16: tools the model may call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<AiTool>,
    /// Wave 16: JSON Schema the answer must follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "kebab-case")]
pub enum AiError {
    #[error("no AI provider is set up")]
    NotConfigured,
    #[error("no API key for this provider")]
    NoKey,
    /// HTTP or transport failure, masked.
    #[error("the provider could not be reached: {0}")]
    Transport(String),
    /// The provider answered with an error (status and its message, masked).
    #[error("the provider refused: {0}")]
    Provider(String),
    /// The model's answer could not be used (e.g. no valid request in it).
    #[error("the answer could not be used: {0}")]
    Answer(String),
    /// Wave 16: the provider, this shell or this build cannot do it (yet), e.g. tool use
    /// with a server that has none.
    #[error("not supported: {0}")]
    Unsupported(String),
    /// Wave 16: the user cancelled the call.
    #[error("cancelled")]
    Cancelled,
    /// Wave 16: the assistant is off for this project (the product's opt-out).
    #[error("the AI assistant is off for this collection")]
    OptedOut,
}

/// A model behind an API. Not `Send` (wasm32).
pub trait AiProvider {
    /// The text of the answer (wave 8).
    fn complete<'a>(&'a self, call: AiCall) -> LocalBoxFuture<'a, Result<String, AiError>>;

    /// Wave 16: the whole answer, with tool calls. The default serves calls without tools
    /// through [`AiProvider::complete`].
    fn reply<'a>(&'a self, call: AiCall) -> LocalBoxFuture<'a, Result<AiReply, AiError>> {
        Box::pin(async move {
            if !call.tools.is_empty() {
                return Err(AiError::Unsupported(
                    "tool use with this provider".to_owned(),
                ));
            }
            let text = self.complete(call).await?;
            Ok(AiReply {
                text,
                ..Default::default()
            })
        })
    }

    /// Wave 16: the answer as it arrives; `on` gets each piece, the result is the whole
    /// answer. The default reports the whole text once, after [`AiProvider::reply`].
    fn stream<'a>(
        &'a self,
        call: AiCall,
        on: &'a dyn Fn(AiDelta),
    ) -> LocalBoxFuture<'a, Result<AiReply, AiError>> {
        Box::pin(async move {
            let reply = self.reply(call).await?;
            if !reply.text.is_empty() {
                on(AiDelta::Text {
                    text: reply.text.clone(),
                });
            }
            for call in &reply.tool_calls {
                on(AiDelta::ToolCall { call: call.clone() });
            }
            Ok(reply)
        })
    }
}

/// Replaces every occurrence of each non-empty secret value with `***` (longest first), the
/// same rule as report masking. Applied to the whole serialized payload before it leaves.
///
/// Besides the raw value it replaces the forms a value takes in text that reaches a model:
/// JSON- and Debug-escaped, percent-encoded (`%20` and `+` for spaces), and base64 (standard
/// and URL-safe, with and without padding; values of 6 characters or more).
pub fn redact(text: &str, secrets: &[String]) -> String {
    mask::redact(text, secrets)
}

/// `e` with every secret in its message replaced by `***`.
pub fn mask_error(e: AiError, secrets: &[String]) -> AiError {
    let m = |s: String| redact(&s, secrets);
    match e {
        AiError::Transport(s) => AiError::Transport(m(s)),
        AiError::Provider(s) => AiError::Provider(m(s)),
        AiError::Answer(s) => AiError::Answer(m(s)),
        other => other,
    }
}

impl AiProviderKind {
    /// The serialized name, e.g. `openai-compatible`; also the key's [`SecretSlot::name`].
    pub fn id(self) -> &'static str {
        match self {
            AiProviderKind::Anthropic => "anthropic",
            AiProviderKind::OpenaiCompatible => "openai-compatible",
            AiProviderKind::Openai => "openai",
            AiProviderKind::AzureOpenai => "azure-openai",
            AiProviderKind::Gemini => "gemini",
            AiProviderKind::Mistral => "mistral",
            AiProviderKind::Bedrock => "bedrock",
        }
    }

    /// Whether calls need a key (`openai-compatible`: optional, local models need none).
    pub fn needs_key(self) -> bool {
        !matches!(self, AiProviderKind::OpenaiCompatible)
    }
}

impl AiConfig {
    /// Checks what the settings dialog lets through: a model name, and a base URL with
    /// `http://` or `https://` (required for `openai-compatible`).
    pub fn validate(&self) -> Result<(), AiError> {
        if self.model.trim().is_empty() {
            return Err(AiError::NotConfigured);
        }
        let url = self.base_url.as_deref().map(str::trim).unwrap_or("");
        if url.is_empty() {
            return if self.provider.capabilities().needs_base_url {
                Err(AiError::NotConfigured)
            } else {
                Ok(())
            };
        }
        if url.starts_with("http://") || url.starts_with("https://") {
            Ok(())
        } else {
            Err(AiError::Transport(format!(
                "the base URL must start with http:// or https://: {url}"
            )))
        }
    }
}

/// Where the key of `provider` is kept in the product's secret store. `space` is the
/// product's own scope for app-wide AI keys; the group is [`KEY_GROUP`], the name the
/// provider's [`AiProviderKind::id`]. Changing any of the three strands stored keys.
pub fn key_slot(space: &str, provider: AiProviderKind) -> SecretSlot {
    SecretSlot {
        space: space.to_owned(),
        group: KEY_GROUP.to_owned(),
        name: provider.id().to_owned(),
    }
}

/// Runs `fut` to completion without a runtime (tests; the future must not need a reactor).
#[cfg(test)]
pub(crate) fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    let mut fut = std::pin::pin!(fut);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_secret_first_and_empty_ignored() {
        let secrets = vec!["ab".to_owned(), "abc".to_owned(), String::new()];
        assert_eq!(redact("x abc ab y", &secrets), "x *** *** y");
    }

    #[test]
    fn config_validation() {
        let mut c = AiConfig {
            provider: AiProviderKind::Anthropic,
            model: DEFAULT_ANTHROPIC_MODEL.into(),
            base_url: None,
        };
        assert_eq!(c.validate(), Ok(()));
        c.model = " ".into();
        assert_eq!(c.validate(), Err(AiError::NotConfigured));
        c.model = "llama3.1".into();
        c.provider = AiProviderKind::OpenaiCompatible;
        assert_eq!(c.validate(), Err(AiError::NotConfigured));
        c.base_url = Some("localhost:11434".into());
        assert!(matches!(c.validate(), Err(AiError::Transport(_))));
        c.base_url = Some("http://localhost:11434/v1".into());
        assert_eq!(c.validate(), Ok(()));
        assert!(AiProviderKind::Anthropic.needs_key());
        assert!(!AiProviderKind::OpenaiCompatible.needs_key());
        assert_eq!(
            key_slot("app:ai", AiProviderKind::OpenaiCompatible).name,
            "openai-compatible"
        );
    }

    #[test]
    fn key_slots_have_three_fixed_parts() {
        let s = key_slot("app:ai", AiProviderKind::Anthropic);
        assert_eq!(
            (s.space.as_str(), s.group.as_str(), s.name.as_str()),
            ("app:ai", "keys", "anthropic")
        );
        assert_eq!(profile_key_slot("app:ai", "fast").name, "profile:fast");
    }

    #[test]
    fn errors_are_masked() {
        let e = mask_error(
            AiError::Provider("401 bad key topsecret".into()),
            &["topsecret".into()],
        );
        assert_eq!(e, AiError::Provider("401 bad key ***".into()));
        assert_eq!(
            mask_error(AiError::Cancelled, &["x".into()]),
            AiError::Cancelled
        );
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Wave 16 interface: several providers and profiles, what each provider can do, and the
//! per-collection opt-out.
//!
//! - A profile ([`AiProfile`]) is one model behind one provider, e.g. "fast" (a small model)
//!   and "smart" (a large one), possibly on different providers. A task or an extension asks
//!   for a role (`fast`, `smart`) or a profile id; [`AiSettings::resolve`] picks the profile.
//! - The wave-8 configuration (one [`AiConfig`], `ai.json` / `localStorage`) is read as
//!   settings with one profile `default` ([`AiSettings::parse`]); nothing is migrated on disk
//!   until the user saves the settings.
//! - Keys: per profile ([`profile_key_slot`]), falling back to the wave-8 slot of the
//!   provider kind ([`super::key_slot`]), so "fast" and "smart" on one provider share a key
//!   unless the user gives one of them its own.
//! - [`AiCapabilities`]: the provider matrix as data (streaming, tool use, JSON mode, direct
//!   browser calls). The settings dialog and the shells read it; tests keep it equal to the
//!   table in the wave-16 decisions.
//!
//! Owner: wave 16 interface. Implementation: strand A.

use serde::{Deserialize, Serialize};

use super::{AiConfig, AiError, AiProviderKind, KEY_COLLECTION, KEY_ENVIRONMENT};
use crate::SecretKey;

/// Role names a caller may ask for instead of a profile id.
pub const ROLE_FAST: &str = "fast";
pub const ROLE_SMART: &str = "smart";
/// Id of the profile made from a wave-8 configuration.
pub const LEGACY_PROFILE: &str = "default";

/// One model behind one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProfile {
    /// Stable id (`[a-z0-9-]`, at most 32 characters), also the key slot's name.
    pub id: String,
    /// Shown in the settings and the "what will be sent" preview, e.g. "Claude (smart)".
    pub label: String,
    #[serde(flatten)]
    pub config: AiConfig,
}

/// Which profile serves which kind of task.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiRoles {
    /// Short, cheap tasks: names, summaries, explanations of a response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast: Option<String>,
    /// Tasks that need a strong model: scripts, fixes, run analysis, the assistant panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart: Option<String>,
}

/// When the "show what will be sent" preview appears before a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiPreviewMode {
    /// Before the first call of each profile on this device, then on request (default).
    #[default]
    FirstUse,
    /// Before every call.
    Always,
    /// Only on request (the user turned it off after the first use).
    OnRequest,
}

/// The assistant's settings on this device (desktop: `ai.json` in the data folder; web:
/// `localStorage`). No key in here.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiSettings {
    pub profiles: Vec<AiProfile>,
    /// The profile used when neither a role nor an id is asked for; `None`: the first one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub roles: AiRoles,
    #[serde(default)]
    pub preview: AiPreviewMode,
    /// Profiles whose first-use preview the user confirmed on this device.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previewed: Vec<String>,
}

impl AiSettings {
    /// Settings with the wave-8 configuration as the only profile.
    pub fn from_legacy(config: AiConfig) -> Self {
        AiSettings {
            profiles: vec![AiProfile {
                id: LEGACY_PROFILE.to_owned(),
                label: config.provider.label().to_owned(),
                config,
            }],
            ..Default::default()
        }
    }

    /// Reads the stored JSON: wave-16 settings, or a wave-8 [`AiConfig`]. `None` for anything
    /// else (an unreadable file counts as "not set up").
    pub fn parse(json: &[u8]) -> Option<Self> {
        if let Ok(settings) = serde_json::from_slice::<AiSettings>(json) {
            return Some(settings);
        }
        serde_json::from_slice::<AiConfig>(json)
            .ok()
            .map(Self::from_legacy)
    }

    /// The profile for `wanted`: a role (`fast`, `smart`), a profile id, or `None` for the
    /// default. A role without a profile falls back to the default; an unknown id gives
    /// `None`.
    pub fn resolve(&self, wanted: Option<&str>) -> Option<&AiProfile> {
        let by_id = |id: &str| self.profiles.iter().find(|p| p.id == id);
        let default = || {
            self.default_profile
                .as_deref()
                .and_then(by_id)
                .or_else(|| self.profiles.first())
        };
        match wanted {
            None => default(),
            Some(ROLE_FAST) => self.roles.fast.as_deref().and_then(by_id).or_else(default),
            Some(ROLE_SMART) => self.roles.smart.as_deref().and_then(by_id).or_else(default),
            Some(id) => by_id(id),
        }
    }

    /// Checks what the settings dialog lets through: profile ids (`[a-z0-9-]`, 1 to 32
    /// characters, unique), each profile's configuration, and that the default and the roles
    /// name existing profiles.
    pub fn validate(&self) -> Result<(), AiError> {
        let bad = |m: String| AiError::Transport(m);
        for (i, p) in self.profiles.iter().enumerate() {
            let ok_id = !p.id.is_empty()
                && p.id.len() <= 32
                && p.id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !ok_id {
                return Err(bad(format!(
                    "a profile id must be 1 to 32 characters of a-z, 0-9 and -: {}",
                    p.id
                )));
            }
            if self.profiles[..i].iter().any(|q| q.id == p.id) {
                return Err(bad(format!("two profiles have the id {}", p.id)));
            }
            p.config.validate()?;
        }
        let exists = |id: &Option<String>| {
            id.as_ref()
                .is_none_or(|id| self.profiles.iter().any(|p| &p.id == id))
        };
        if !exists(&self.default_profile) || !exists(&self.roles.fast) || !exists(&self.roles.smart)
        {
            return Err(bad(
                "the default profile and the roles must name existing profiles".into(),
            ));
        }
        Ok(())
    }

    /// Keeps `previewed` from `old` for the profiles that still send to the same place (same
    /// provider, model and base URL): a profile that changes where the data goes is a first
    /// use again. Whatever `previewed` the caller sent is discarded.
    pub fn carry_previewed(&mut self, old: Option<&AiSettings>) {
        let same = |p: &AiProfile| {
            old.is_some_and(|o| {
                o.previewed.contains(&p.id)
                    && o.profiles
                        .iter()
                        .any(|q| q.id == p.id && q.config == p.config)
            })
        };
        self.previewed = self
            .profiles
            .iter()
            .filter(|p| same(p))
            .map(|p| p.id.clone())
            .collect();
    }

    /// The user confirmed the first-use preview of `profile`.
    pub fn confirm(&mut self, profile: &str) {
        if self.profiles.iter().any(|p| p.id == profile)
            && !self.previewed.iter().any(|p| p == profile)
        {
            self.previewed.push(profile.to_owned());
        }
    }

    /// The wave-8 view of the settings: the default profile's configuration.
    pub fn default_config(&self) -> Option<&AiConfig> {
        self.resolve(None).map(|p| &p.config)
    }

    /// Whether the preview must be shown before a call of `profile`.
    pub fn must_preview(&self, profile: &str) -> bool {
        match self.preview {
            AiPreviewMode::Always => true,
            AiPreviewMode::FirstUse => !self.previewed.iter().any(|p| p == profile),
            AiPreviewMode::OnRequest => false,
        }
    }
}

/// Where the key of a profile is kept (before the provider kind's slot, [`super::key_slot`]).
pub fn profile_key_slot(profile: &str) -> SecretKey {
    SecretKey {
        collection: KEY_COLLECTION.to_owned(),
        environment: KEY_ENVIRONMENT.to_owned(),
        variable: format!("profile:{profile}"),
    }
}

/// How a provider can return JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiJsonMode {
    /// The API enforces a JSON schema (structured outputs).
    Schema,
    /// The API guarantees a JSON object, not its shape.
    Object,
    /// Only by asking in the prompt (and the fenced-block parser of `answer`).
    Prompt,
}

/// What a provider can do, as the shells use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiCapabilities {
    pub streaming: bool,
    pub tools: bool,
    pub json: AiJsonMode,
    /// A web page may call it with `fetch` (CORS allowed); otherwise the web app goes
    /// through the companion (`rt agent`).
    pub browser_direct: bool,
    /// The settings dialog requires a base URL (endpoint of the resource or server).
    pub needs_base_url: bool,
}

impl AiProviderKind {
    /// Every kind, in the order the settings dialog lists them.
    pub const ALL: &[AiProviderKind] = &[
        AiProviderKind::Anthropic,
        AiProviderKind::Openai,
        AiProviderKind::AzureOpenai,
        AiProviderKind::Gemini,
        AiProviderKind::Mistral,
        AiProviderKind::OpenaiCompatible,
        AiProviderKind::Bedrock,
    ];

    /// Name in the settings dialog.
    pub fn label(self) -> &'static str {
        match self {
            AiProviderKind::Anthropic => "Anthropic",
            AiProviderKind::Openai => "OpenAI",
            AiProviderKind::AzureOpenai => "Azure OpenAI",
            AiProviderKind::Gemini => "Google Gemini",
            AiProviderKind::Mistral => "Mistral",
            AiProviderKind::OpenaiCompatible => "OpenAI-compatible endpoint",
            AiProviderKind::Bedrock => "AWS Bedrock",
        }
    }

    /// The provider matrix (wave 16 decisions). `openai-compatible` promises the least: each
    /// server differs, so tool use and JSON are tried and fall back to the prompt.
    pub fn capabilities(self) -> AiCapabilities {
        let c = |streaming, tools, json, browser_direct, needs_base_url| AiCapabilities {
            streaming,
            tools,
            json,
            browser_direct,
            needs_base_url,
        };
        match self {
            AiProviderKind::Anthropic => c(true, true, AiJsonMode::Schema, true, false),
            AiProviderKind::Openai => c(true, true, AiJsonMode::Schema, false, false),
            AiProviderKind::AzureOpenai => c(true, true, AiJsonMode::Schema, false, true),
            AiProviderKind::Gemini => c(true, true, AiJsonMode::Schema, true, false),
            AiProviderKind::Mistral => c(true, true, AiJsonMode::Schema, false, false),
            AiProviderKind::OpenaiCompatible => c(true, false, AiJsonMode::Prompt, false, true),
            AiProviderKind::Bedrock => c(true, true, AiJsonMode::Prompt, false, true),
        }
    }
}

/// Whether the assistant may be used for a collection. Off if either says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiCollectionPolicy {
    /// `extensions.requesttailor.ai.enabled: false` in `opencollection.yml`: the team decided
    /// that this repository never goes to an AI provider.
    pub file_off: bool,
    /// The user turned it off for this collection on this device.
    pub device_off: bool,
}

impl AiCollectionPolicy {
    pub fn allowed(self) -> bool {
        !self.file_off && !self.device_off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, provider: AiProviderKind, model: &str) -> AiProfile {
        AiProfile {
            id: id.into(),
            label: id.into(),
            config: AiConfig {
                provider,
                model: model.into(),
                base_url: None,
            },
        }
    }

    #[test]
    fn a_wave8_config_reads_as_one_profile() {
        let json = br#"{"provider":"anthropic","model":"claude-opus-5-5","base_url":null}"#;
        let s = AiSettings::parse(json).unwrap();
        assert_eq!(s.profiles.len(), 1);
        assert_eq!(s.profiles[0].id, LEGACY_PROFILE);
        assert_eq!(
            s.resolve(Some(ROLE_SMART)).unwrap().config.model,
            "claude-opus-5-5"
        );
        assert!(s.must_preview(LEGACY_PROFILE));
        assert!(AiSettings::parse(b"not json").is_none());
        let back = serde_json::to_vec(&s).unwrap();
        assert_eq!(AiSettings::parse(&back), Some(s));
    }

    #[test]
    fn roles_and_ids_resolve() {
        let s = AiSettings {
            profiles: vec![
                profile("smart", AiProviderKind::Anthropic, "claude-opus-5-5"),
                profile("local", AiProviderKind::OpenaiCompatible, "llama3.1"),
            ],
            default_profile: Some("smart".into()),
            roles: AiRoles {
                fast: Some("local".into()),
                smart: None,
            },
            previewed: vec!["smart".into()],
            ..Default::default()
        };
        assert_eq!(s.resolve(None).unwrap().id, "smart");
        assert_eq!(s.resolve(Some(ROLE_FAST)).unwrap().id, "local");
        assert_eq!(s.resolve(Some(ROLE_SMART)).unwrap().id, "smart");
        assert_eq!(s.resolve(Some("local")).unwrap().id, "local");
        assert!(s.resolve(Some("missing")).is_none());
        assert!(!s.must_preview("smart"));
        assert!(s.must_preview("local"));
        assert_eq!(profile_key_slot("local").variable, "profile:local");
    }

    #[test]
    fn settings_are_checked_and_a_moved_profile_is_a_first_use_again() {
        let mut s = AiSettings {
            profiles: vec![
                profile("a", AiProviderKind::Anthropic, "m1"),
                profile("b", AiProviderKind::Openai, "m2"),
            ],
            roles: AiRoles {
                fast: Some("b".into()),
                smart: Some("a".into()),
            },
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.profiles.push(profile("a", AiProviderKind::Gemini, "x"));
        assert!(s.validate().is_err());
        s.profiles.pop();
        s.profiles
            .push(profile("Bad Id", AiProviderKind::Gemini, "x"));
        assert!(s.validate().is_err());
        s.profiles.pop();
        s.roles.fast = Some("gone".into());
        assert!(s.validate().is_err());
        s.roles.fast = None;
        s.profiles
            .push(profile("c", AiProviderKind::OpenaiCompatible, "m"));
        assert!(s.validate().is_err(), "a compatible endpoint needs its URL");

        let mut old = AiSettings {
            profiles: vec![
                profile("a", AiProviderKind::Anthropic, "m1"),
                profile("b", AiProviderKind::Openai, "m2"),
            ],
            ..Default::default()
        };
        old.confirm("a");
        old.confirm("b");
        old.confirm("nope");
        assert_eq!(old.previewed, ["a", "b"]);
        let mut next = old.clone();
        next.previewed = vec!["a".into(), "b".into(), "forged".into()];
        next.profiles[1].config.model = "other".into();
        next.carry_previewed(Some(&old));
        assert_eq!(next.previewed, ["a"]);
        assert!(!next.must_preview("a") && next.must_preview("b"));
    }

    #[test]
    fn every_kind_has_a_label_and_the_opt_out_wins() {
        for kind in AiProviderKind::ALL {
            assert!(!kind.label().is_empty());
            let _ = kind.capabilities();
        }
        assert!(AiCollectionPolicy::default().allowed());
        assert!(
            !AiCollectionPolicy {
                file_off: true,
                device_off: false
            }
            .allowed()
        );
    }
}

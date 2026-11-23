// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 16 interface: the providers the settings dialog offers and what each can do, the same
// table as `rt_app::ai::AiProviderKind::{ALL, label, capabilities}` (a test compares it with the
// contract fixture). Owner: strand A.

import type { AiCapabilities, AiProviderKind } from "../commands/types";

export interface AiProviderInfo {
  kind: AiProviderKind;
  label: string;
  capabilities: AiCapabilities;
}

const caps = (
  streaming: boolean,
  tools: boolean,
  json: AiCapabilities["json"],
  browser_direct: boolean,
  needs_base_url: boolean,
): AiCapabilities => ({ streaming, tools, json, browser_direct, needs_base_url });

/** In the order of `AiProviderKind::ALL`. */
export const AI_PROVIDERS: readonly AiProviderInfo[] = [
  { kind: "anthropic", label: "Anthropic", capabilities: caps(true, true, "schema", true, false) },
  { kind: "openai", label: "OpenAI", capabilities: caps(true, true, "schema", true, false) },
  { kind: "azure-openai", label: "Azure OpenAI", capabilities: caps(true, true, "schema", false, true) },
  { kind: "gemini", label: "Google Gemini", capabilities: caps(true, true, "schema", true, false) },
  { kind: "mistral", label: "Mistral", capabilities: caps(true, true, "schema", false, false) },
  { kind: "openai-compatible", label: "OpenAI-compatible endpoint", capabilities: caps(true, false, "prompt", false, true) },
  { kind: "bedrock", label: "AWS Bedrock", capabilities: caps(true, true, "prompt", false, true) },
];

export function providerInfo(kind: AiProviderKind): AiProviderInfo {
  const found = AI_PROVIDERS.find((p) => p.kind === kind);
  if (!found) throw new Error(`unknown AI provider ${kind}`);
  return found;
}

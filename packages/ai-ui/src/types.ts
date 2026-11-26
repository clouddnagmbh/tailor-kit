// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// The AI wire types the UI shares with the Rust side (`tk-ai`: `tk_ai::*`). Moved here from
// RequestTailor's `commands/types.ts`, which re-exports them; shapes unchanged. The scope of a call
// (what of the product's content goes into it) belongs to the product, so `AiRequest` takes it as
// a type parameter.

export type AiProviderKind =
  | "anthropic"
  | "openai-compatible"
  // Wave 16:
  | "openai"
  | "azure-openai"
  | "gemini"
  | "mistral"
  | "bedrock";

export interface AiConfig {
  provider: AiProviderKind;
  model: string;
  base_url: string | null;
}

/** Wave 16: one model behind one provider (`rt_app::ai::AiProfile`, the config flattened). */
export interface AiProfile extends AiConfig {
  id: string;
  label: string;
}

export interface AiRoles {
  fast?: string;
  smart?: string;
}

export type AiPreviewMode = "first-use" | "always" | "on-request";

/** Wave 16: the assistant's settings on this device; never a key. */
export interface AiSettings {
  profiles: AiProfile[];
  default_profile?: string;
  roles: AiRoles;
  preview: AiPreviewMode;
  previewed?: string[];
}

export type AiJsonMode = "schema" | "object" | "prompt";

/** Wave 16: what a provider can do (`AiProviderKind::capabilities`). */
export interface AiCapabilities {
  streaming: boolean;
  tools: boolean;
  json: AiJsonMode;
  browser_direct: boolean;
  needs_base_url: boolean;
}

/** Wave 16: whether the assistant may be used here; off if either is set (RequestTailor: the
 *  active collection, `AiCollectionPolicy`). */
export interface AiPolicy {
  file_off: boolean;
  device_off: boolean;
}

export type AiError =
  | { kind: "not-configured" }
  | { kind: "no-key" }
  | { kind: "transport"; message: string }
  | { kind: "provider"; message: string }
  | { kind: "answer"; message: string }
  // Wave 16:
  | { kind: "unsupported"; message: string }
  | { kind: "cancelled" }
  | { kind: "opted-out" };

export type AiRole = "user" | "assistant";

/** Wave 16: a tool the model may call; `input_schema` is a JSON Schema. */
export interface AiTool {
  name: string;
  description: string;
  input_schema: unknown;
}

export interface AiToolCall {
  id: string;
  name: string;
  input: unknown;
}

export interface AiToolResult {
  call_id: string;
  content: string;
  is_error?: boolean;
}

export interface AiMessage {
  role: AiRole;
  content: string;
  tool_calls?: AiToolCall[];
  tool_results?: AiToolResult[];
}

/** One model call, provider-neutral, masked. */
export interface AiCall {
  system: string;
  messages: AiMessage[];
  max_tokens: number;
  tools?: AiTool[];
  json?: unknown;
}

/** Wave 16: a call an extension asks for (`ExtensionContext.ai`). `S` is the product's scope of
 *  the masked context (`tk_ai::AiRequest<S>`). */
export interface AiRequest<S = unknown> {
  /** Chosen by the caller; `AiApi.cancel(id)` stops the call. */
  id: string;
  /** One line for the preview: what the call is for. */
  purpose: string;
  /** A role (`fast`, `smart`) or a profile id; absent: the default profile. */
  profile?: string;
  system: string;
  messages: AiMessage[];
  scope: S;
  tools?: AiTool[];
  /** JSON Schema the answer must follow. */
  json?: unknown;
  max_tokens?: number;
}

/** Wave 16: exactly what a call will send, masked; nothing has left yet. */
export interface AiPreview {
  profile: string;
  provider: AiProviderKind;
  model: string;
  endpoint: string;
  call: AiCall;
  masked: number;
  bytes: number;
  /** The settings ask for this preview to be confirmed before the call. */
  required: boolean;
}

export type AiStop = "end" | "max-tokens" | "tool-use" | "refusal";

export interface AiUsage {
  input_tokens: number;
  output_tokens: number;
}

export interface AiReply {
  text: string;
  tool_calls?: AiToolCall[];
  stop: AiStop;
  usage?: AiUsage;
}

export type AiDelta = { kind: "text"; text: string } | { kind: "tool-call"; call: AiToolCall };

/** Wave 16: what a streaming call reports while it runs. */
export type AiEvent =
  | { kind: "delta"; delta: AiDelta }
  | { kind: "done"; reply: AiReply }
  | { kind: "error"; error: AiError };

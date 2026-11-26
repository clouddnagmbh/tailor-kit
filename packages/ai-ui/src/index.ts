// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

export { default as AiDialog } from "./AiDialog.svelte";
export { default as Markdown } from "./Markdown.svelte";
export { default as PreviewDialog } from "./PreviewDialog.svelte";
export { default as SettingsDialog } from "./SettingsDialog.svelte";
export {
  createAiClient,
  dialogConfirm,
  nextCallId,
  type AiClientApi,
  type AiClientOptions,
  type AiDialogSpec,
  type ConfirmPreview,
  type ExtensionAi,
  type ExtensionAiOptions,
  type ExtensionAiProfile,
  type ExtensionAiRequest,
  type ExtensionAiStatus,
} from "./extension-ai";
export { getAiHost, setAiHost, type AiBrand, type AiHost, type AiSettingsApi } from "./host";
export { parseMarkdown, type Block as MarkdownBlock, type Span as MarkdownSpan } from "./markdown";
export { segments, type Segment } from "./preview";
export { AI_PROVIDERS, providerInfo, type AiProviderInfo } from "./providers";
export type * from "./types";

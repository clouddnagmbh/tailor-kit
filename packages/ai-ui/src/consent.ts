// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Text of the consent popover (wave 8): what one call will send, in one line. Pure, so products and
// tests share the wording.

import type { AiPreview } from "./types";

/** `api.anthropic.com` from an endpoint URL; the endpoint itself when it has no host part. */
export function previewHost(endpoint: string): string {
  return endpoint.replace(/^[a-z]+:\/\//i, "").split("/")[0] ?? endpoint;
}

/** `316 B`, `1.2 KB`, `3.4 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** "To api.anthropic.com · claude-sonnet · 316 B · 1 secret masked". */
export function consentLine(preview: Pick<AiPreview, "endpoint" | "model" | "bytes" | "masked">): string {
  const masked = preview.masked === 0 ? "no secrets" : `${preview.masked} secret${preview.masked === 1 ? "" : "s"} masked`;
  return `To ${previewHost(preview.endpoint)} · ${preview.model} · ${formatBytes(preview.bytes)} · ${masked}`;
}

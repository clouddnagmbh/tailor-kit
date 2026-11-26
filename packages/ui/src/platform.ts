// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Platform sniffing shared by the primitives (key labels, tooltips) and the host's shortcuts.

export function isMacPlatform(nav: { platform?: string; userAgent?: string } | undefined = globalThis.navigator): boolean {
  const p = nav?.platform || nav?.userAgent || "";
  return /Mac|iPhone|iPad|iPod/.test(p);
}

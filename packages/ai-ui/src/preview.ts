// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// Helpers of the "what will be sent" dialog (wave 16, strand A).

export interface Segment {
  text: string;
  /** A place where a secret value was replaced by `***`. */
  masked: boolean;
}

/** `text` split at the `***` marks the core left in place of secret values. */
export function segments(text: string): Segment[] {
  const out: Segment[] = [];
  const parts = text.split("***");
  parts.forEach((part, i) => {
    if (part !== "") out.push({ text: part, masked: false });
    if (i < parts.length - 1) out.push({ text: "***", masked: true });
  });
  return out;
}

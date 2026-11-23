// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Initials and a stable colour for a person (commit authors, reviewers). Pure.

/** Up to two initials: "Ada Lovelace" → "AL", "ada.lovelace@example.com" → "AL", "ada" → "AD". */
export function initials(name: string): string {
  const base = name.trim().split("@")[0] ?? "";
  const words = base.split(/[\s._-]+/).filter((w) => /\p{L}|\p{N}/u.test(w));
  if (words.length === 0) return "?";
  if (words.length === 1) return [...words[0]].slice(0, 2).join("").toLocaleUpperCase();
  return ([...words[0]][0] + [...words[words.length - 1]][0]).toLocaleUpperCase();
}

/** One of `count` colours (1-based), the same for the same name on every device (FNV-1a). */
export function colourIndex(name: string, count = 8): number {
  let h = 0x811c9dc5;
  for (const ch of name.trim().toLocaleLowerCase()) {
    h ^= ch.codePointAt(0)!;
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return (h % count) + 1;
}

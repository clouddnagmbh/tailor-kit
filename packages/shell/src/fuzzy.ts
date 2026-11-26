// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Fuzzy filter for the command palette. The query is split at whitespace; every part must
// appear in the text as a subsequence (case-insensitive). Matches at word starts and runs of
// consecutive characters score higher.

function scoreToken(token: string, text: string): number | null {
  let score = 0;
  let from = 0;
  let prev = -2;
  for (const ch of token) {
    const i = text.indexOf(ch, from);
    if (i < 0) return null;
    score += 1;
    if (i === prev + 1) score += 3;
    if (i === 0 || /[\s:/._-]/.test(text[i - 1])) score += 5;
    prev = i;
    from = i + 1;
  }
  // Earlier matches rank slightly higher.
  return score - text.indexOf(token[0]) * 0.01;
}

/** Score of `text` for `query`, or `null` if it does not match. Empty query matches all. */
export function fuzzyScore(query: string, text: string): number | null {
  const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
  const lower = text.toLowerCase();
  let total = 0;
  for (const t of tokens) {
    const s = scoreToken(t, lower);
    if (s === null) return null;
    total += s;
  }
  return total;
}

/** Items matching `query`, best first; ties keep their original order. */
export function fuzzyFilter<T>(items: readonly T[], query: string, text: (item: T) => string): T[] {
  return items
    .map((item, index) => ({ item, index, score: fuzzyScore(query, text(item)) }))
    .filter((x): x is { item: T; index: number; score: number } => x.score !== null)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((x) => x.item);
}

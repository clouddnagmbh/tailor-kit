// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// The +/- bar of a changed file (wave 8, strand C, V7): a few small cells, green for added and
// red for removed lines, in proportion, with a text form for screen readers. No DOM here, so
// `bar.test.ts` covers it.

import type { GitDiff } from "../types";

export interface LineCounts {
  added: number;
  removed: number;
}

/** Added and removed lines of a diff (context lines do not count). */
export function diffCounts(diff: Pick<GitDiff, "hunks">): LineCounts {
  let added = 0;
  let removed = 0;
  for (const h of diff.hunks) {
    for (const l of h.lines) {
      if (l.kind === "added") added++;
      else if (l.kind === "removed") removed++;
    }
  }
  return { added, removed };
}

export interface BarCells {
  add: number;
  del: number;
  /** Empty cells (so every bar is as wide as the others). */
  rest: number;
}

/** `cells` small boxes split by the share of added and removed lines. A side with any lines
 *  gets at least one cell; a small change does not fill the bar (one cell per started ten
 *  lines); nothing changed gives all empty cells. */
export function barCells(counts: LineCounts, cells = 5): BarCells {
  const total = counts.added + counts.removed;
  if (total === 0 || cells <= 0) return { add: 0, del: 0, rest: Math.max(0, cells) };
  const both = counts.added > 0 && counts.removed > 0;
  const used = Math.min(cells, Math.max(both ? 2 : 1, Math.ceil(total / 10)));
  if (counts.removed === 0) return { add: used, del: 0, rest: cells - used };
  if (counts.added === 0) return { add: 0, del: used, rest: cells - used };
  const add = Math.min(used - 1, Math.max(1, Math.round((counts.added / total) * used)));
  return { add, del: used - add, rest: cells - used };
}

/** "+12 −3", for the screen reader and the tooltip. */
export function barText(counts: LineCounts): string {
  if (counts.added === 0 && counts.removed === 0) return "no line changes";
  const parts: string[] = [];
  if (counts.added > 0) parts.push(`${counts.added} ${counts.added === 1 ? "line" : "lines"} added`);
  if (counts.removed > 0) parts.push(`${counts.removed} ${counts.removed === 1 ? "line" : "lines"} removed`);
  return parts.join(", ");
}

/** "+12 −3" as the short visible form. */
export function barShort(counts: LineCounts): string {
  return `+${counts.added} −${counts.removed}`;
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Line diffs for the semantic diff view (wave 12, strand A): the inline code diff of a
// multi-line field (a body, a script), and the line diff of a file at two revisions.

import type { DiffHunk, DiffLine, GitDiff } from "../types";

export interface LineOp {
  kind: "same" | "added" | "removed";
  text: string;
}

/** Above this many cells of the LCS table the diff is "everything removed, everything added". */
const MAX_CELLS = 4_000_000;

export function splitLines(text: string | null): string[] {
  return text ? text.replace(/\r?\n$/, "").split(/\r?\n/) : [];
}

/** Line-by-line differences of `old` → `next`, in order. */
export function lineOps(old: string | null, next: string | null): LineOp[] {
  const a = splitLines(old);
  const b = splitLines(next);
  // The common start and end need no table.
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head++;
  let tail = 0;
  while (tail < a.length - head && tail < b.length - head && a[a.length - 1 - tail] === b[b.length - 1 - tail]) tail++;
  const am = a.slice(head, a.length - tail);
  const bm = b.slice(head, b.length - tail);
  const ops: LineOp[] = a.slice(0, head).map((text) => ({ kind: "same", text }));
  if ((am.length + 1) * (bm.length + 1) > MAX_CELLS) {
    ops.push(...am.map((text): LineOp => ({ kind: "removed", text })), ...bm.map((text): LineOp => ({ kind: "added", text })));
  } else {
    const lcs = Array.from({ length: am.length + 1 }, () => new Array<number>(bm.length + 1).fill(0));
    for (let i = am.length - 1; i >= 0; i--) {
      for (let j = bm.length - 1; j >= 0; j--) {
        lcs[i][j] = am[i] === bm[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < am.length || j < bm.length) {
      if (i < am.length && j < bm.length && am[i] === bm[j]) {
        ops.push({ kind: "same", text: am[i] });
        i++;
        j++;
      } else if (i < am.length && (j === bm.length || lcs[i + 1][j] >= lcs[i][j + 1])) {
        ops.push({ kind: "removed", text: am[i++] });
      } else {
        ops.push({ kind: "added", text: bm[j++] });
      }
    }
  }
  ops.push(...a.slice(a.length - tail).map((text): LineOp => ({ kind: "same", text })));
  return ops;
}

/** The hunks (with `context` lines around changes) of `old` → `next`, in the shape of
 *  `rt_app::git::text_diff`. */
export function lineDiff(path: string, old: string | null, next: string | null, context = 3): GitDiff {
  const ops = lineOps(old, next);
  let oldAt = 0;
  let newAt = 0;
  const placed = ops.map((op) => {
    const at = { op, oldAt, newAt };
    if (op.kind !== "added") oldAt++;
    if (op.kind !== "removed") newAt++;
    return at;
  });
  const changed = placed.flatMap((p, k) => (p.op.kind === "same" ? [] : [k]));
  const hunks: DiffHunk[] = [];
  let k = 0;
  while (k < changed.length) {
    const from = Math.max(0, changed[k] - context);
    let to = changed[k];
    while (k + 1 < changed.length && changed[k + 1] - to <= 2 * context) to = changed[++k];
    to = Math.min(placed.length - 1, to + context);
    k++;
    const slice = placed.slice(from, to + 1);
    const oldLines = slice.filter((p) => p.op.kind !== "added").length;
    const newLines = slice.filter((p) => p.op.kind !== "removed").length;
    const first = slice[0];
    hunks.push({
      old_start: oldLines === 0 ? first.oldAt : first.oldAt + 1,
      old_lines: oldLines,
      new_start: newLines === 0 ? first.newAt : first.newAt + 1,
      new_lines: newLines,
      lines: slice.map((p): DiffLine => ({ kind: p.op.kind === "same" ? "context" : p.op.kind, text: p.op.text })),
    });
  }
  return { path, binary: false, hunks };
}

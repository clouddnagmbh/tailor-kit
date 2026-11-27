// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Line diffs for the semantic diff view (wave 12, strand A): the inline code diff of a
// multi-line field (a body, a script), and the line diff of a file at two revisions.

import type { DiffHunk, DiffLine, GitDiff } from "../types";

export interface LineOp {
  kind: "same" | "added" | "removed";
  text: string;
}

/** Above this many cells of the LCS table the diff is anchored on the lines that occur once on
 *  both sides (patience diff) and only the stretches between anchors use a table. */
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
    for (const op of anchoredOps(am, bm)) ops.push(op);
  } else {
    const lcs = Array.from({ length: am.length + 1 }, () => new Array<number>(bm.length + 1).fill(0));
    for (let i = am.length - 1; i >= 0; i--) {
      const cur = lcs[i];
      const nxt = lcs[i + 1];
      if (!cur || !nxt) continue;
      for (let j = bm.length - 1; j >= 0; j--) {
        cur[j] = am[i] === bm[j] ? (nxt[j + 1] ?? 0) + 1 : Math.max(nxt[j] ?? 0, cur[j + 1] ?? 0);
      }
    }
    let i = 0;
    let j = 0;
    while (i < am.length || j < bm.length) {
      const left = am[i];
      const right = bm[j];
      if (left !== undefined && right !== undefined && left === right) {
        ops.push({ kind: "same", text: left });
        i++;
        j++;
      } else if (left !== undefined && (j === bm.length || (lcs[i + 1]?.[j] ?? 0) >= (lcs[i]?.[j + 1] ?? 0))) {
        ops.push({ kind: "removed", text: left });
        i++;
      } else if (right !== undefined) {
        ops.push({ kind: "added", text: right });
        j++;
      } else {
        break;
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
    const start = changed[k] ?? 0;
    const from = Math.max(0, start - context);
    let to = start;
    while (k + 1 < changed.length && (changed[k + 1] ?? 0) - to <= 2 * context) to = changed[++k] ?? to;
    to = Math.min(placed.length - 1, to + context);
    k++;
    const slice = placed.slice(from, to + 1);
    const oldLines = slice.filter((p) => p.op.kind !== "added").length;
    const newLines = slice.filter((p) => p.op.kind !== "removed").length;
    const first = slice[0];
    if (!first) continue;
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

/**
 * The diff of two long line lists: lines that occur exactly once on both sides anchor the
 * match (the longest run that keeps its order), and every stretch between anchors is done the
 * same way, or with a table once it is small. Lines of a stretch without anchors are removed
 * and added whole. Linear in the number of lines for files with scattered edits, which is
 * what a 10 MB file is.
 */
export function anchoredOps(am: string[], bm: string[]): LineOp[] {
  const ids = new Map<string, number>();
  const id = (line: string): number => {
    let n = ids.get(line);
    if (n === undefined) {
      n = ids.size;
      ids.set(line, n);
    }
    return n;
  };
  const a = Int32Array.from(am, id);
  const b = Int32Array.from(bm, id);
  // Matched positions, as pairs (old, new) in order.
  const pairs: number[] = [];
  matchRange(a, b, 0, a.length, 0, b.length, pairs);
  const ops: LineOp[] = [];
  let i = 0;
  let j = 0;
  const flush = (toI: number, toJ: number) => {
    while (i < toI) ops.push({ kind: "removed", text: am[i++] ?? "" });
    while (j < toJ) ops.push({ kind: "added", text: bm[j++] ?? "" });
  };
  for (let k = 0; k < pairs.length; k += 2) {
    flush(pairs[k] ?? 0, pairs[k + 1] ?? 0);
    ops.push({ kind: "same", text: am[i] ?? "" });
    i++;
    j++;
  }
  flush(am.length, bm.length);
  return ops;
}

function matchRange(a: Int32Array, b: Int32Array, aFrom: number, aTo: number, bFrom: number, bTo: number, out: number[]): void {
  while (aFrom < aTo && bFrom < bTo && a[aFrom] === b[bFrom]) {
    out.push(aFrom++, bFrom++);
  }
  const tail: number[] = [];
  while (aFrom < aTo && bFrom < bTo && a[aTo - 1] === b[bTo - 1]) {
    aTo--;
    bTo--;
    tail.push(aTo, bTo);
  }
  const n = aTo - aFrom;
  const m = bTo - bFrom;
  if (n > 0 && m > 0) {
    if ((n + 1) * (m + 1) <= MAX_CELLS) tableMatch(a, b, aFrom, aTo, bFrom, bTo, out);
    else patience(a, b, aFrom, aTo, bFrom, bTo, out);
  }
  for (let k = tail.length - 2; k >= 0; k -= 2) out.push(tail[k] ?? 0, tail[k + 1] ?? 0);
}

function tableMatch(a: Int32Array, b: Int32Array, aFrom: number, aTo: number, bFrom: number, bTo: number, out: number[]): void {
  const n = aTo - aFrom;
  const m = bTo - bFrom;
  const w = m + 1;
  const table = new Uint32Array((n + 1) * w);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i * w + j] =
        a[aFrom + i] === b[bFrom + j]
          ? (table[(i + 1) * w + j + 1] ?? 0) + 1
          : Math.max(table[(i + 1) * w + j] ?? 0, table[i * w + j + 1] ?? 0);
    }
  }
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[aFrom + i] === b[bFrom + j]) {
      out.push(aFrom + i++, bFrom + j++);
    } else if ((table[(i + 1) * w + j] ?? 0) >= (table[i * w + j + 1] ?? 0)) {
      i++;
    } else {
      j++;
    }
  }
}

function patience(a: Int32Array, b: Int32Array, aFrom: number, aTo: number, bFrom: number, bTo: number, out: number[]): void {
  // How often each line occurs on each side, and where it is on the new side.
  const countA = new Map<number, number>();
  for (let i = aFrom; i < aTo; i++) countA.set(a[i] ?? 0, (countA.get(a[i] ?? 0) ?? 0) + 1);
  const countB = new Map<number, number>();
  const where = new Map<number, number>();
  for (let j = bFrom; j < bTo; j++) {
    const key = b[j] ?? 0;
    countB.set(key, (countB.get(key) ?? 0) + 1);
    where.set(key, j);
  }
  // Anchors in old order; the longest increasing run of their new positions.
  const anchorI: number[] = [];
  const anchorJ: number[] = [];
  for (let i = aFrom; i < aTo; i++) {
    const key = a[i] ?? 0;
    if (countA.get(key) === 1 && countB.get(key) === 1) {
      anchorI.push(i);
      anchorJ.push(where.get(key) ?? 0);
    }
  }
  const tails: number[] = [];
  const prev = new Int32Array(anchorI.length).fill(-1);
  for (let k = 0; k < anchorI.length; k++) {
    let lo = 0;
    let hi = tails.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if ((anchorJ[tails[mid] ?? 0] ?? 0) < (anchorJ[k] ?? 0)) lo = mid + 1;
      else hi = mid;
    }
    prev[k] = lo > 0 ? (tails[lo - 1] ?? -1) : -1;
    tails[lo] = k;
  }
  const chain: number[] = [];
  for (let k = tails.length > 0 ? (tails[tails.length - 1] ?? -1) : -1; k >= 0; k = prev[k] ?? -1) chain.push(k);
  chain.reverse();
  let ia = aFrom;
  let jb = bFrom;
  for (const k of chain) {
    const i = anchorI[k] ?? 0;
    const j = anchorJ[k] ?? 0;
    matchRange(a, b, ia, i, jb, j, out);
    out.push(i, j);
    ia = i + 1;
    jb = j + 1;
  }
  if (chain.length > 0) matchRange(a, b, ia, aTo, jb, bTo, out);
}

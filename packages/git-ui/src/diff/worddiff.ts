// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Word diff (wave 14, strand H): the parts of two strings that differ, for the word
// highlighting of changed values and text bodies (`--rt-diff-*-word`). Words, runs of spaces
// and single punctuation marks are the units, so `/v1/users` → `/v2/users` marks only `v1`.

import { lineOps } from "./textdiff";

export interface Segment {
  text: string;
  /** Part of the other side too. */
  same: boolean;
}

export interface WordDiff {
  old: Segment[];
  new: Segment[];
}

/** Above this many cells of the table the strings are marked as a whole. */
const MAX_CELLS = 250_000;

export function tokens(text: string): string[] {
  return text.match(/[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]/gu) ?? [];
}

function push(out: Segment[], text: string, same: boolean) {
  const last = out[out.length - 1];
  if (last && last.same === same) last.text += text;
  else if (text !== "") out.push({ text, same });
}

/** The segments of `old` and `next`, each side in order; joined they give the strings back. */
export function wordDiff(old: string, next: string): WordDiff {
  const a = tokens(old);
  const b = tokens(next);
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head++;
  let tail = 0;
  while (tail < a.length - head && tail < b.length - head && a[a.length - 1 - tail] === b[b.length - 1 - tail]) tail++;
  const am = a.slice(head, a.length - tail);
  const bm = b.slice(head, b.length - tail);
  const out: WordDiff = { old: [], new: [] };
  const start = a.slice(0, head).join("");
  push(out.old, start, true);
  push(out.new, start, true);
  if ((am.length + 1) * (bm.length + 1) > MAX_CELLS) {
    push(out.old, am.join(""), false);
    push(out.new, bm.join(""), false);
  } else {
    const lcs = Array.from({ length: am.length + 1 }, () => new Array<number>(bm.length + 1).fill(0));
    for (let i = am.length - 1; i >= 0; i--) {
      for (let j = bm.length - 1; j >= 0; j--) {
        lcs[i]![j] = am[i] === bm[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
      }
    }
    let i = 0;
    let j = 0;
    while (i < am.length || j < bm.length) {
      if (i < am.length && j < bm.length && am[i] === bm[j]) {
        push(out.old, am[i]!, true);
        push(out.new, bm[j]!, true);
        i++;
        j++;
      } else if (i < am.length && (j === bm.length || lcs[i + 1]![j]! >= lcs[i]![j + 1]!)) {
        push(out.old, am[i++]!, false);
      } else {
        push(out.new, bm[j++]!, false);
      }
    }
  }
  const end = a.slice(a.length - tail).join("");
  push(out.old, end, true);
  push(out.new, end, true);
  return out;
}

/** One row of a side-by-side text diff: a line on each side (`null`: none there). */
export interface TextRow {
  kind: "same" | "changed" | "added" | "removed";
  old: Segment[] | null;
  new: Segment[] | null;
  oldLine: number | null;
  newLine: number | null;
}

/**
 * Two texts side by side: equal lines face each other, a run of removed lines faces the run of
 * added lines after it (pairwise, with the words that differ marked), the rest stands alone.
 */
export function textRows(old: string, next: string): TextRow[] {
  const ops = lineOps(old, next);
  const rows: TextRow[] = [];
  let oldLine = 0;
  let newLine = 0;
  let k = 0;
  while (k < ops.length) {
    const op = ops[k]!;
    if (op.kind === "same") {
      oldLine++;
      newLine++;
      rows.push({ kind: "same", old: [{ text: op.text, same: true }], new: [{ text: op.text, same: true }], oldLine, newLine });
      k++;
      continue;
    }
    const removed: string[] = [];
    const added: string[] = [];
    while (k < ops.length && ops[k]!.kind === "removed") removed.push(ops[k++]!.text);
    while (k < ops.length && ops[k]!.kind === "added") added.push(ops[k++]!.text);
    for (let i = 0; i < Math.max(removed.length, added.length); i++) {
      const [o, n] = [removed[i], added[i]];
      if (o !== undefined && n !== undefined) {
        const w = wordDiff(o, n);
        rows.push({ kind: "changed", old: w.old, new: w.new, oldLine: ++oldLine, newLine: ++newLine });
      } else if (o !== undefined) {
        rows.push({ kind: "removed", old: [{ text: o, same: false }], new: null, oldLine: ++oldLine, newLine: null });
      } else {
        rows.push({ kind: "added", old: null, new: [{ text: n!, same: false }], oldLine: null, newLine: ++newLine });
      }
    }
  }
  return rows;
}

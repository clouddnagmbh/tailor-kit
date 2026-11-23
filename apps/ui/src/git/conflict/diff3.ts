// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand D: the 3-way merge of a text file as blocks, for `TextMergeView`. Lines that
// both sides keep (or only one side changed) are `same` blocks; where both changed the same
// part differently there is a `conflict` block with all three versions. This only splits and
// joins lines for the view; the file Git writes is whatever `GitApi.resolveText` is given.

export type Choice = "ours" | "theirs" | "both" | "base";

export type Block = { kind: "same"; lines: string[] } | { kind: "conflict"; base: string[]; ours: string[]; theirs: string[] };

/** `text` as lines; each line keeps no newline, the final newline is remembered separately. */
export function toLines(text: string | null): string[] {
  if (!text) return [];
  return text.replace(/\n$/, "").split("\n");
}

/** Pairs `[i, j]` of equal lines in `a` and `b`, a longest common subsequence. */
function lcs(a: string[], b: string[]): [number, number][] {
  const n = a.length;
  const m = b.length;
  const table: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  const out: [number, number][] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push([i, j]);
      i++;
      j++;
    } else if (table[i + 1][j] >= table[i][j + 1]) i++;
    else j++;
  }
  return out;
}

const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);

/** The three-way merge of `ours` and `theirs` over `base`, as blocks. */
export function diff3(base: string[], ours: string[], theirs: string[]): Block[] {
  const toOurs = new Map(lcs(base, ours));
  const toTheirs = new Map(lcs(base, theirs));
  // Base lines both sides keep are the anchors between the changed regions.
  const anchors: [number, number, number][] = [];
  for (let b = 0; b < base.length; b++) {
    const o = toOurs.get(b);
    const t = toTheirs.get(b);
    if (o !== undefined && t !== undefined) anchors.push([b, o, t]);
  }
  anchors.push([base.length, ours.length, theirs.length]);
  const blocks: Block[] = [];
  const push = (block: Block) => {
    const last = blocks[blocks.length - 1];
    if (block.kind === "same" && last?.kind === "same") last.lines.push(...block.lines);
    else if (block.kind === "same" && block.lines.length === 0) return;
    else blocks.push(block);
  };
  let [pb, po, pt] = [0, 0, 0];
  for (const [b, o, t] of anchors) {
    const gb = base.slice(pb, b);
    const go = ours.slice(po, o);
    const gt = theirs.slice(pt, t);
    if (same(go, gt)) push({ kind: "same", lines: go });
    else if (same(gb, go)) push({ kind: "same", lines: gt });
    else if (same(gb, gt)) push({ kind: "same", lines: go });
    else push({ kind: "conflict", base: gb, ours: go, theirs: gt });
    if (b < base.length) push({ kind: "same", lines: [base[b]] });
    [pb, po, pt] = [b + 1, o + 1, t + 1];
  }
  return blocks;
}

/** The lines a choice puts in the file. */
export function pick(block: Extract<Block, { kind: "conflict" }>, choice: Choice): string[] {
  switch (choice) {
    case "ours":
      return block.ours;
    case "theirs":
      return block.theirs;
    case "both":
      return [...block.ours, ...block.theirs];
    case "base":
      return block.base;
  }
}

/** The file with `choices[i]` for the i-th conflict (default: ours), ending in a newline. */
export function join(blocks: Block[], choices: Choice[]): string {
  let n = 0;
  const lines: string[] = [];
  for (const block of blocks) {
    if (block.kind === "same") lines.push(...block.lines);
    else lines.push(...pick(block, choices[n++] ?? "ours"));
  }
  return lines.length === 0 ? "" : `${lines.join("\n")}\n`;
}

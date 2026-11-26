// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// What the file viewers (wave 15, strand B) decide without a DOM: how a blob is shown, a line
// diff with the words that changed marked, sizes and the fields of a Git LFS pointer.

import { wordDiff, type Segment } from "../diff/worddiff";
import type { GitBlob } from "../types";
import { lineOps } from "../diff/textdiff";

/** One side of a comparison: a file at a revision (`rev` `null`: the working tree). */
export interface Side {
  rev: string | null;
  path: string;
}

/** How a file is shown: lines of text, an image, a Git LFS pointer, or only what is known. */
export type BlobKind = "text" | "image" | "lfs" | "binary" | "large";

export function blobKind(blob: GitBlob): BlobKind {
  if (blob.lfs_pointer) return "lfs";
  if (blob.base64 !== undefined && blob.base64 !== null && blob.media_type?.startsWith("image/")) return "image";
  if (blob.text !== undefined && blob.text !== null) return "text";
  return blob.truncated ? "large" : "binary";
}

/** `1.5 KB`, `12 MB`: binary units written the usual short way. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

/** The size change as a short text ("+1.2 KB", "-300 B", "same size"). */
export function sizeChange(before: number, after: number): string {
  if (before === after) return "same size";
  const diff = Math.abs(after - before);
  return `${after > before ? "+" : "-"}${formatSize(diff)}`;
}

/** `data:` URL of an image blob. */
export function dataUrl(blob: GitBlob): string {
  return `data:${blob.media_type ?? "application/octet-stream"};base64,${blob.base64 ?? ""}`;
}

export interface LfsPointer {
  oid: string;
  size: number | null;
}

/** The `oid sha256:…` and `size …` lines of a Git LFS pointer file; `null` if it is not one. */
export function parseLfsPointer(text: string | null | undefined): LfsPointer | null {
  if (!text || !/^version https:\/\/git-lfs\.github\.com\/spec\/v1\s/.test(text)) return null;
  const oid = /^oid (\S+)$/m.exec(text)?.[1];
  if (!oid) return null;
  const size = /^size (\d+)$/m.exec(text)?.[1];
  return { oid, size: size === undefined ? null : Number(size) };
}

/** Short form of an object id for display. */
export function shortId(id: string | null | undefined, length = 10): string {
  if (!id) return "";
  return id.replace(/^[a-z0-9]+:/, "").slice(0, length);
}

export type DiffRow =
  | { kind: "hunk"; text: string }
  | { kind: "context" | "added" | "removed"; old: number | null; new: number | null; text: string; words?: Segment[] };

const CONTEXT = 3;
/** Pairs of changed lines are compared word by word up to this many per run. */
const MAX_PAIRED = 200;

/**
 * The lines that differ between `before` and `after` with three lines of context, grouped in
 * hunks. A run of removed lines followed by a run of added lines is paired line by line and the
 * words that differ are marked (`words`), as the viewers show them.
 */
export function diffRows(before: string | null, after: string | null, context = CONTEXT): DiffRow[] {
  const ops = lineOps(before, after);
  let oldAt = 0;
  let newAt = 0;
  const placed = ops.map((op) => {
    const at = { ...op, old: op.kind === "added" ? null : ++oldAt, new: op.kind === "removed" ? null : ++newAt };
    return at;
  });
  const changed = placed.flatMap((p, k) => (p.kind === "same" ? [] : [k]));
  const rows: DiffRow[] = [];
  let k = 0;
  while (k < changed.length) {
    const from = Math.max(0, changed[k]! - context);
    let to = changed[k]!;
    while (k + 1 < changed.length && changed[k + 1]! - to <= 2 * context) to = changed[++k]!;
    to = Math.min(placed.length - 1, to + context);
    k++;
    const slice = placed.slice(from, to + 1);
    const first = slice[0]!;
    const oldLines = slice.filter((p) => p.old !== null).length;
    const newLines = slice.filter((p) => p.new !== null).length;
    rows.push({
      kind: "hunk",
      text: `@@ -${oldLines === 0 ? (first.old ?? 1) - 1 : (first.old ?? 1)},${oldLines} +${newLines === 0 ? (first.new ?? 1) - 1 : (first.new ?? 1)},${newLines} @@`,
    });
    for (const p of slice) {
      rows.push({ kind: p.kind === "same" ? "context" : p.kind, old: p.old, new: p.new, text: p.text });
    }
  }
  markWords(rows);
  return rows;
}

/** Marks the words of paired removed/added lines in place. */
function markWords(rows: DiffRow[]): void {
  let i = 0;
  while (i < rows.length) {
    if (rows[i]!.kind !== "removed") {
      i++;
      continue;
    }
    let r = i;
    while (r < rows.length && rows[r]!.kind === "removed") r++;
    let a = r;
    while (a < rows.length && rows[a]!.kind === "added") a++;
    const pairs = Math.min(r - i, a - r, MAX_PAIRED);
    for (let p = 0; p < pairs; p++) {
      const removed = rows[i + p]! as Extract<DiffRow, { text: string; old: number | null }>;
      const added = rows[r + p]! as Extract<DiffRow, { text: string; old: number | null }>;
      const diff = wordDiff(removed.text, added.text);
      // Nothing in common: the whole line differs, which marking every word would only repeat.
      if (diff.old.every((s) => !s.same) || diff.new.every((s) => !s.same)) continue;
      removed.words = diff.old;
      added.words = diff.new;
    }
    i = a;
  }
}

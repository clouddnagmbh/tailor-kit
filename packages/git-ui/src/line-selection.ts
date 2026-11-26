// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand F: choosing lines of a diff. A `GitHunkSelection` indexes the hunks and lines
// of the diff `GitIndexDiff.diff` carries; this maps them onto the two texts and builds the text
// with exactly the chosen changes applied (`invert` false: the old text with them, as `git add
// -p` writes the index) or taken back (`invert` true: the new text without them, as `git reset
// -p` and `git checkout -p` do). The same as `crates/rt-git/src/patch.rs`; the mock and the web
// app use it, the desktop app does it in Rust.
//
// Unchosen removals stay as context and unchosen additions are dropped. Lines keep their own
// line endings; the diff's lines are matched by position (`old_start`, `new_start`).

import { BackendError } from "./api";
import type { GitDiff, GitHunkSelection } from "./types";

/** `text` as lines that keep their line break (the last one may have none). */
export function splitLines(text: string): string[] {
  return text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
}

const key = (hunk: number, line: number) => `${hunk}:${line}`;

/** The changed lines `selection` names, as `hunk:line` keys. Context lines are ignored; a hunk
 *  or line that does not exist is refused. */
export function chosenLines(diff: GitDiff, selection: GitHunkSelection[]): Set<string> {
  const out = new Set<string>();
  for (const pick of selection) {
    const hunk = diff.hunks[pick.hunk];
    if (!hunk) throw new BackendError("git", `The diff has no hunk ${pick.hunk}.`);
    const wanted = pick.lines ?? hunk.lines.map((_, i) => i);
    for (const i of wanted) {
      const line = hunk.lines[i];
      if (!line) throw new BackendError("git", `Hunk ${pick.hunk} has no line ${i}.`);
      if (line.kind !== "context") out.add(key(pick.hunk, i));
    }
  }
  return out;
}

/** The first index of a side of a hunk (an empty side starts after the line it names). */
const startOf = (start: number, lines: number) => (lines === 0 ? start : start - 1);

export function applyLineSelection(
  oldText: string,
  newText: string,
  diff: GitDiff,
  chosen: Set<string>,
  invert = false,
): string {
  const old = splitLines(oldText);
  const next = splitLines(newText);
  const out: string[] = [];
  // Walk the side the result is built on (old text, or new text when inverted) and the other
  // one next to it.
  let po = 0;
  let pn = 0;
  diff.hunks.forEach((hunk, h) => {
    const oi = startOf(hunk.old_start, hunk.old_lines);
    const ni = startOf(hunk.new_start, hunk.new_lines);
    // Untouched lines between hunks are the same on both sides.
    if (invert) {
      out.push(...next.slice(pn, ni));
    } else {
      out.push(...old.slice(po, oi));
    }
    po = oi;
    pn = ni;
    hunk.lines.forEach((line, l) => {
      const picked = chosen.has(key(h, l));
      if (line.kind === "context") {
        out.push(invert ? next[pn] : old[po]);
        po++;
        pn++;
      } else if (line.kind === "removed") {
        // Applied: the line goes; taken back: it comes back.
        if (invert ? picked : !picked) out.push(old[po]);
        po++;
      } else {
        // Applied: the line comes; taken back: it goes.
        if (invert ? !picked : picked) out.push(next[pn]);
        pn++;
      }
    });
  });
  out.push(...(invert ? next.slice(pn) : old.slice(po)));
  return out.join("");
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { anchoredOps, lineDiff, lineOps } from "./textdiff";

describe("lineOps", () => {
  it("marks removed, added and unchanged lines in order", () => {
    expect(lineOps("a\nb\nc\n", "a\nx\nc\n")).toEqual([
      { kind: "same", text: "a" },
      { kind: "removed", text: "b" },
      { kind: "added", text: "x" },
      { kind: "same", text: "c" },
    ]);
  });

  it("handles a missing side and CRLF", () => {
    expect(lineOps(null, "a\r\nb")).toEqual([
      { kind: "added", text: "a" },
      { kind: "added", text: "b" },
    ]);
    expect(lineOps("a", null)).toEqual([{ kind: "removed", text: "a" }]);
    expect(lineOps("a\r\nb\r\n", "a\nb\n").every((o) => o.kind === "same")).toBe(true);
  });
});

describe("lineDiff", () => {
  it("builds hunks with context and 1-based starts", () => {
    const old = Array.from({ length: 20 }, (_, i) => `l${i + 1}`).join("\n");
    const next = old.replace("l10", "changed");
    const d = lineDiff("f.yml", old, next);
    expect(d.hunks).toHaveLength(1);
    const h = d.hunks[0];
    if (!h) throw new Error("no hunk");
    expect([h.old_start, h.old_lines, h.new_start, h.new_lines]).toEqual([7, 7, 7, 7]);
    expect(h.lines.filter((l) => l.kind !== "context").map((l) => [l.kind, l.text])).toEqual([
      ["removed", "l10"],
      ["added", "changed"],
    ]);
  });

  it("is empty for equal texts and splits far-apart changes", () => {
    expect(lineDiff("f", "a\nb\n", "a\nb\n").hunks).toEqual([]);
    const old = Array.from({ length: 30 }, (_, i) => `l${i}`).join("\n");
    const next = old.replace("l2\n", "x\n").replace("l27\n", "y\n");
    expect(lineDiff("f", old, next).hunks).toHaveLength(2);
  });

  it("diffs a long file with scattered edits line by line, not as one removal", () => {
    const n = 200_000;
    const lines = Array.from({ length: n }, (_, i) => `line ${i} of the file`);
    const edited = lines.slice();
    for (let i = 1000; i < n; i += 25_000) edited[i] = `changed ${i}`;
    edited.splice(100_000, 3); // three lines deleted
    edited.splice(150_000, 0, "new a", "new b"); // two lines inserted
    const started = performance.now();
    const ops = lineOps(lines.join("\n"), edited.join("\n"));
    expect(performance.now() - started).toBeLessThan(3000);
    const changed = ops.filter((o) => o.kind !== "same");
    // 8 replaced lines (removed + added), 3 deleted, 2 inserted: a few lines, not 400 000.
    expect(changed.filter((o) => o.kind === "removed")).toHaveLength(8 + 3);
    expect(changed.filter((o) => o.kind === "added")).toHaveLength(8 + 2);
    // Both sides come back whole.
    expect(ops.filter((o) => o.kind !== "added").map((o) => o.text)).toEqual(lines);
    expect(ops.filter((o) => o.kind !== "removed").map((o) => o.text)).toEqual(edited);
  });

  it("falls back to whole-side changes where no line is unique", () => {
    const a = Array.from({ length: 5000 }, (_, i) => `x${i % 2}`);
    const b = Array.from({ length: 5000 }, (_, i) => `y${i % 2}`);
    const ops = anchoredOps(a, b);
    expect(ops.filter((o) => o.kind === "removed")).toHaveLength(5000);
    expect(ops.filter((o) => o.kind === "added")).toHaveLength(5000);
  });
});

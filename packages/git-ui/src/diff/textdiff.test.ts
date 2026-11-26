// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { lineDiff, lineOps } from "./textdiff";

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
});

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { diff3, join, toLines } from "./diff3";

const L = (s: string) => toLines(s);

describe("diff3", () => {
  it("merges changes on different lines without a conflict", () => {
    const blocks = diff3(L("a\nb\nc\nd\n"), L("A\nb\nc\nd\n"), L("a\nb\nc\nD\n"));
    expect(blocks).toEqual([{ kind: "same", lines: ["A", "b", "c", "D"] }]);
  });

  it("finds a conflict where both sides changed the same line", () => {
    const blocks = diff3(L("a\nb\nc\n"), L("a\nB ours\nc\n"), L("a\nB theirs\nc\n"));
    expect(blocks).toEqual([
      { kind: "same", lines: ["a"] },
      { kind: "conflict", base: ["b"], ours: ["B ours"], theirs: ["B theirs"] },
      { kind: "same", lines: ["c"] },
    ]);
    expect(join(blocks, ["ours"])).toBe("a\nB ours\nc\n");
    expect(join(blocks, ["theirs"])).toBe("a\nB theirs\nc\n");
    expect(join(blocks, ["both"])).toBe("a\nB ours\nB theirs\nc\n");
    expect(join(blocks, ["base"])).toBe("a\nb\nc\n");
  });

  it("takes an identical change from both sides once", () => {
    expect(join(diff3(L("a\nb\n"), L("a\nX\n"), L("a\nX\n")), [])).toBe("a\nX\n");
  });

  it("handles a deletion against an edit as a conflict, and added files with no base", () => {
    const blocks = diff3(L("a\nb\nc\n"), L("a\nc\n"), L("a\nB\nc\n"));
    expect(blocks.filter((b) => b.kind === "conflict")).toHaveLength(1);
    const added = diff3([], L("x\n"), L("y\n"));
    expect(added).toEqual([{ kind: "conflict", base: [], ours: ["x"], theirs: ["y"] }]);
    expect(join([], [])).toBe("");
  });
});

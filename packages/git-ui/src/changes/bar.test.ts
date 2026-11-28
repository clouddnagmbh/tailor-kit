// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { barCells, barShort, barText, diffCounts } from "./bar";

describe("change bar", () => {
  it("counts added and removed lines, not context", () => {
    const diff = {
      hunks: [
        { old_start: 1, old_lines: 3, new_start: 1, new_lines: 3, lines: [{ kind: "context", text: "a" }, { kind: "removed", text: "b" }, { kind: "added", text: "c" }, { kind: "added", text: "d" }] },
      ],
    } as const;
    expect(diffCounts(diff as never)).toEqual({ added: 2, removed: 1 });
  });

  it("always has as many cells as asked", () => {
    for (const [added, removed] of [[0, 0], [1, 0], [0, 1], [3, 3], [100, 1], [1, 100], [7, 2]] as const) {
      const c = barCells({ added, removed }, 5);
      expect(c.add + c.del + c.rest).toBe(5);
      if (added > 0) expect(c.add).toBeGreaterThan(0);
      if (removed > 0) expect(c.del).toBeGreaterThan(0);
      if (added === 0) expect(c.add).toBe(0);
      if (removed === 0) expect(c.del).toBe(0);
    }
  });

  it("a one line change is one cell, a large one fills the bar", () => {
    expect(barCells({ added: 1, removed: 0 })).toEqual({ add: 1, del: 0, rest: 4 });
    const big = barCells({ added: 200, removed: 200 });
    expect(big.rest).toBe(0);
  });

  it("has a text form", () => {
    expect(barText({ added: 12, removed: 1 })).toBe("12 lines added, 1 line removed");
    expect(barText({ added: 0, removed: 0 })).toBe("no line changes");
    expect(barShort({ added: 12, removed: 3 })).toBe("+12 −3");
  });
});

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { squarify } from "./treemap";

describe("squarified treemap", () => {
  const items = [
    { id: "a", size: 60 },
    { id: "b", size: 30 },
    { id: "c", size: 6 },
    { id: "d", size: 4 },
    { id: "zero", size: 0 },
  ];
  const rects = squarify(items, 100, 60);

  it("gives every item with a size an area in proportion to it, filling the rectangle", () => {
    expect(rects.map((r) => r.id).sort()).toEqual(["a", "b", "c", "d"]);
    const total = rects.reduce((n, r) => n + r.w * r.h, 0);
    expect(total).toBeCloseTo(6000, 6);
    for (const r of rects) expect((r.w * r.h) / total).toBeCloseTo(items.find((i) => i.id === r.id)!.size / 100, 6);
  });

  it("stays inside the rectangle and never overlaps", () => {
    for (const r of rects) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-9);
      expect(r.y).toBeGreaterThanOrEqual(-1e-9);
      expect(r.x + r.w).toBeLessThanOrEqual(100 + 1e-9);
      expect(r.y + r.h).toBeLessThanOrEqual(60 + 1e-9);
    }
    for (let i = 0; i < rects.length; i++)
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i]!;
        const b = rects[j]!;
        const overlapX = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
        const overlapY = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
        expect(overlapX > 1e-9 && overlapY > 1e-9).toBe(false);
      }
  });

  it("handles nothing, one item and many", () => {
    expect(squarify([], 10, 10)).toEqual([]);
    expect(squarify([{ id: "x", size: 5 }], 10, 4)).toEqual([{ id: "x", x: 0, y: 0, w: 10, h: 4 }]);
    const many = squarify(Array.from({ length: 300 }, (_, i) => ({ id: `f${i}`, size: 1 + (i % 17) })), 100, 60);
    expect(many).toHaveLength(300);
  });
});

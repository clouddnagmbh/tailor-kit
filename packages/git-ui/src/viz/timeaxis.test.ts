// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { dateTicks, placeMarks } from "./timeaxis";

describe("time axis", () => {
  it("places marks in time order and keeps them apart", () => {
    const { placed, width } = placeMarks(
      [
        { id: "b", time: 1000 },
        { id: "a", time: 0 },
        { id: "c", time: 1001 },
        { id: "d", time: 1002 },
      ],
      400,
      10,
      16,
    );
    expect(placed.map((p) => p.id)).toEqual(["a", "b", "c", "d"]);
    expect(placed[0]!.x).toBe(16);
    for (let i = 1; i < placed.length; i++) expect(placed[i]!.x - placed[i - 1]!.x).toBeGreaterThanOrEqual(10);
    expect(width).toBeGreaterThanOrEqual(400);
    expect(placeMarks([], 300).placed).toEqual([]);
  });

  it("widens the axis when a burst needs room", () => {
    const burst = Array.from({ length: 100 }, (_, i) => ({ id: `m${i}`, time: 5000 + i }));
    const { width } = placeMarks([{ id: "first", time: 0 }, ...burst], 300, 9);
    expect(width).toBeGreaterThan(900);
  });

  it("picks round ticks, few of them", () => {
    const t = dateTicks(1_700_000_000, 1_700_000_000 + 5 * 86_400);
    expect(t.length).toBeGreaterThan(2);
    expect(t.length).toBeLessThanOrEqual(8);
    const hourly = dateTicks(1_700_000_000, 1_700_000_000 + 3 * 3600);
    expect(hourly[0]!.label).toMatch(/^\d\d:\d\d$/);
  });
});

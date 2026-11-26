// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { fitTabs } from "./tab-fit";

const base = { min: 100, max: 200, moreWidth: 60 };

describe("fitTabs", () => {
  it("shows nothing for no tabs", () => {
    expect(fitTabs({ ...base, count: 0, active: -1, available: 500 })).toMatchObject({ visible: [], hidden: [] });
  });

  it("shares the width up to the maximum", () => {
    const f = fitTabs({ ...base, count: 2, active: 0, available: 1000 });
    expect(f).toEqual({ visible: [0, 1], hidden: [], width: 200 });
  });

  it("shrinks tabs before hiding any", () => {
    const f = fitTabs({ ...base, count: 5, active: 0, available: 600 });
    expect(f.visible).toEqual([0, 1, 2, 3, 4]);
    expect(f.width).toBe(120);
  });

  it("hides what does not fit behind the overflow button", () => {
    const f = fitTabs({ ...base, count: 12, active: 1, available: 560 });
    // (560 - 60) / 100 = 5 slots
    expect(f.visible).toEqual([0, 1, 2, 3, 4]);
    expect(f.hidden).toEqual([5, 6, 7, 8, 9, 10, 11]);
    expect(f.width).toBe(100);
  });

  it("gives a hidden active tab the last visible slot", () => {
    const f = fitTabs({ ...base, count: 12, active: 9, available: 560 });
    expect(f.visible).toEqual([0, 1, 2, 3, 9]);
    expect(f.hidden).toEqual([4, 5, 6, 7, 8, 10, 11]);
  });

  it("keeps the active tab even when only one slot is left", () => {
    const f = fitTabs({ ...base, count: 4, active: 3, available: 120 });
    expect(f.visible).toEqual([3]);
    expect(f.hidden).toEqual([0, 1, 2]);
  });

  it("shows everything while the strip is not measured", () => {
    expect(fitTabs({ ...base, count: 9, active: 0, available: 0 }).hidden).toEqual([]);
  });

  it("never loses a tab", () => {
    for (let count = 1; count < 20; count++) {
      for (let active = 0; active < count; active++) {
        const f = fitTabs({ ...base, count, active, available: 400 });
        expect([...f.visible, ...f.hidden].sort((a, b) => a - b)).toEqual(Array.from({ length: count }, (_, i) => i));
        expect(f.visible).toContain(active);
      }
    }
  });
});

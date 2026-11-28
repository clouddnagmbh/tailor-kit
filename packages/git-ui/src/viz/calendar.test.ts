// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { calendarCells, dayNumber, dayString, monthLabels, weekdayOf } from "./calendar";
import { ageStep, heatColor, heatLevels } from "./color";

describe("calendar grid", () => {
  it("knows the weekday (Monday 0) and round-trips days", () => {
    expect(weekdayOf(dayNumber("2023-11-13"))).toBe(0);
    expect(weekdayOf(dayNumber("2023-11-19"))).toBe(6);
    expect(dayString(dayNumber("2024-02-29"))).toBe("2024-02-29");
  });

  it("ends with the week of the last day and holds only days up to it", () => {
    const cells = calendarCells([{ day: "2023-11-15", count: 3 }], "2023-11-16", 4);
    expect(cells[0]).toMatchObject({ day: "2023-10-23", week: 0, row: 0 });
    expect(cells.at(-1)).toMatchObject({ day: "2023-11-16", week: 3, row: 3 });
    expect(cells.find((c) => c.day === "2023-11-15")!.count).toBe(3);
    expect(cells).toHaveLength(3 * 7 + 4);
  });

  it("labels each month once, never closer than three weeks", () => {
    const cells = calendarCells([], "2024-03-31", 53);
    const labels = monthLabels(cells);
    expect(new Set(labels.map((l) => l.label)).size).toBe(labels.length);
    expect(labels.length).toBeGreaterThanOrEqual(11);
    for (let i = 1; i < labels.length; i++) expect(labels[i]!.week - labels[i - 1]!.week).toBeGreaterThanOrEqual(3);
  });
});

describe("colour steps", () => {
  it("shows the busiest day in the full tint even in a calm repository", () => {
    const level = heatLevels([1, 1, 2]);
    expect(level(0)).toBe(0);
    expect(level(1)).toBe(2);
    expect(level(2)).toBe(4);
    expect(heatColor(0)).toContain("surface-sunken");
    expect(heatColor(4)).toContain("100%");
    expect(heatLevels([])(5)).toBe(0);
  });
  it("steps the age of a line", () => {
    expect([0, 6, 7, 29, 30, 90, 91, 181, 182, 364, 365, 900].map(ageStep)).toEqual([0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5]);
  });
});

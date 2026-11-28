// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { fuzzyFilter, fuzzyPositions, fuzzyScore } from "./fuzzy";

describe("fuzzy", () => {
  it("matches subsequences per word and ranks word starts first", () => {
    expect(fuzzyScore("sndreq", "Send request")).not.toBeNull();
    expect(fuzzyScore("xyz", "Send request")).toBeNull();
    expect(fuzzyScore("env prd", "Environment: PRD")).not.toBeNull();
    expect(fuzzyScore("env prd", "Environment: Local")).toBeNull();
    const labels = ["Close tab", "Save", "Send request", "Toggle theme"];
    expect(fuzzyFilter(labels, "s", (x) => x)[0]).toBe("Save");
    expect(fuzzyFilter(labels, "", (x) => x)).toEqual(labels);
  });

  it("reports the matched characters for highlighting", () => {
    expect(fuzzyPositions("log", "feature/login")).toEqual([8, 9, 10]);
    expect(fuzzyPositions("fl", "feature/login")).toEqual([0, 8]);
    expect(fuzzyPositions("xyz", "feature/login")).toBeNull();
    expect(fuzzyPositions("", "feature/login")).toEqual([]);
    expect(fuzzyPositions("fe log", "feature/login")).toEqual([0, 1, 8, 9, 10]);
  });
});

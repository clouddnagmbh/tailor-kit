// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { textRows, tokens, wordDiff } from "./worddiff";

const changed = (segs: { text: string; same: boolean }[]) => segs.filter((s) => !s.same).map((s) => s.text);
const join = (segs: { text: string }[]) => segs.map((s) => s.text).join("");

describe("word diff", () => {
  it("splits into words, spaces and marks", () => {
    expect(tokens("GET /v1/users?id=7")).toEqual(["GET", " ", "/", "v1", "/", "users", "?", "id", "=", "7"]);
    expect(tokens("Grüße 42")).toEqual(["Grüße", " ", "42"]);
  });

  it("marks only the words that differ", () => {
    const w = wordDiff("/v1/users/7", "/v2/users/7");
    expect(changed(w.old)).toEqual(["v1"]);
    expect(changed(w.new)).toEqual(["v2"]);
    expect(join(w.old)).toBe("/v1/users/7");
    expect(join(w.new)).toBe("/v2/users/7");
  });

  it("handles insertions, deletions and equal strings", () => {
    const w = wordDiff("the quick fox", "the quick brown fox");
    expect(changed(w.old)).toEqual([]);
    expect(changed(w.new)).toEqual(["brown "]);
    expect(wordDiff("same", "same")).toEqual({ old: [{ text: "same", same: true }], new: [{ text: "same", same: true }] });
    expect(wordDiff("", "new")).toEqual({ old: [], new: [{ text: "new", same: false }] });
  });

  it("marks very long strings as a whole", () => {
    const a = Array.from({ length: 800 }, (_, i) => `a${i}`).join(" ");
    const b = Array.from({ length: 800 }, (_, i) => `b${i}`).join(" ");
    const w = wordDiff(a, b);
    expect(w.old).toEqual([{ text: a, same: false }]);
  });
});

describe("text side by side", () => {
  it("pairs removed and added lines and numbers both sides", () => {
    const rows = textRows("one\ntwo\nthree\n", "one\ntoo\nthree\nfour\n");
    expect(rows.map((r) => r.kind)).toEqual(["same", "changed", "same", "added"]);
    expect(rows[1]).toMatchObject({ oldLine: 2, newLine: 2 });
    expect(changed(rows[1]!.old!)).toEqual(["two"]);
    expect(rows[3]).toMatchObject({ old: null, oldLine: null, newLine: 4 });
    const gone = textRows("a\nb\n", "a\n");
    expect(gone.map((r) => r.kind)).toEqual(["same", "removed"]);
  });
});

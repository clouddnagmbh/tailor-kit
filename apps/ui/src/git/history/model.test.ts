// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitBlame, GitReflogEntry } from "../../commands/types";
import { blameLines, commitCount, EMPTY_FIELDS, fieldsOf, recoveredName, reflogRows, textLines, toQuery } from "./model";

const commit = (oid: string) => ({ oid, summary: oid, author: "Ada", time: 1 });

describe("history model", () => {
  it("splits a text into lines like blame counts them", () => {
    expect(textLines("")).toEqual([]);
    expect(textLines("a\nb\n")).toEqual(["a", "b"]);
    expect(textLines("a\nb")).toEqual(["a", "b"]);
    expect(textLines("\n")).toEqual([""]);
  });

  it("pairs every line with its hunk, marks the first line and alternates the bands", () => {
    const blame: GitBlame = {
      path: "/a.txt",
      rev: "r",
      lines: 3,
      binary: false,
      hunks: [
        { start: 1, lines: 2, commit: commit("c1"), path: "/a.txt", orig_start: 1 },
        { start: 3, lines: 1, commit: commit("c2"), path: "/a.txt", orig_start: 3 },
      ],
    };
    const rows = blameLines(blame, ["x", "y"]);
    expect(rows.map((r) => [r.n, r.text, r.first, r.band])).toEqual([
      [1, "x", true, 0],
      [2, "y", false, 0],
      [3, "", true, 1],
    ]);
    expect(commitCount(blame)).toBe(2);
  });

  it("makes a query from the fields and back, and says when nothing is asked", () => {
    expect(toQuery(EMPTY_FIELDS)).toBeNull();
    expect(toQuery({ ...EMPTY_FIELDS, message: "  fix  ", path: "api" })).toEqual({ message: "fix", path: "/api" });
    expect(toQuery({ ...EMPTY_FIELDS, author: "ada", since: "2026-01-01", until: "2026-01-02" })).toEqual({
      author: "ada",
      since: Date.UTC(2026, 0, 1) / 1000,
      until: Date.UTC(2026, 0, 2, 23, 59, 59) / 1000,
    });
    expect(fieldsOf({ content: "token", since: Date.UTC(2026, 0, 5) / 1000 })).toMatchObject({ content: "token", since: "2026-01-05", until: "" });
  });

  it("marks commits no branch reaches as lost, or unknown when only part of the history was read", () => {
    const entries = [{ new: "a" }, { new: "b" }] as GitReflogEntry[];
    expect(reflogRows(entries, new Set(["a"]), true).map((r) => r.lost)).toEqual([false, true]);
    expect(reflogRows(entries, new Set(["a"]), false).map((r) => r.lost)).toEqual([false, null]);
    expect(recoveredName({ new: "0123456789abcdef" })).toBe("recovered-0123456");
  });
});

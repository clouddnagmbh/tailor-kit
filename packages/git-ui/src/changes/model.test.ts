// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitWorkingFile } from "../types";
import { countStates, fileName, filterFiles, folderOf, groupByFolder, NO_FILTER, pathsOf, statusLetter, summaryText, visibleFiles } from "./model";

const f = (path: string, state: GitWorkingFile["state"], more: Partial<GitWorkingFile> = {}): GitWorkingFile => ({ path, state, unstaged: true, ...more });

const FILES = [
  f("/api/b.yml", "modified"),
  f("/README.md", "modified"),
  f("/api/a.yml", "untracked"),
  f("/old/x.yml", "deleted"),
  f("/new/x.yml", "untracked", { from: "/old/x.yml" }),
  f("/c.txt", "conflicted"),
];

describe("changes model", () => {
  it("folds the deleted half of a rename into the new row", () => {
    expect(visibleFiles(FILES).map((x) => x.path)).not.toContain("/old/x.yml");
    expect(pathsOf(FILES[4]!)).toEqual(["/new/x.yml", "/old/x.yml"]);
    expect(pathsOf(FILES[0]!)).toEqual(["/api/b.yml"]);
  });

  it("filters by text, state and staged", () => {
    expect(filterFiles(FILES, { ...NO_FILTER, text: "API/" }).map((x) => x.path)).toEqual(["/api/b.yml", "/api/a.yml"]);
    expect(filterFiles(FILES, { ...NO_FILTER, state: "deleted" })).toHaveLength(1);
    expect(filterFiles(FILES, { ...NO_FILTER, text: "old/" }).map((x) => x.path)).toContain("/new/x.yml");
    const staged = [f("/s", "modified", { staged: "modified" }), f("/u", "modified")];
    expect(filterFiles(staged, { ...NO_FILTER, staged: true }).map((x) => x.path)).toEqual(["/s"]);
  });

  it("counts and summarizes", () => {
    const counts = countStates(visibleFiles(FILES));
    expect(counts).toEqual({ all: 5, untracked: 2, modified: 2, deleted: 0, conflicted: 1 });
    expect(summaryText(counts)).toBe("2 modified, 2 new, 1 in conflict");
    expect(summaryText(countStates([]))).toBe("No changes");
  });

  it("groups by folder, top level first", () => {
    const groups = groupByFolder(visibleFiles(FILES));
    expect(groups.map((g) => g.folder)).toEqual(["/", "/api", "/new"]);
    expect(groups[1]!.files.map((x) => x.path)).toEqual(["/api/a.yml", "/api/b.yml"]);
    expect(folderOf("/a/b/c.yml")).toBe("/a/b");
    expect(folderOf("/c.yml")).toBe("/");
    expect(fileName("/a/b/c.yml")).toBe("c.yml");
  });

  it("picks the status letter", () => {
    expect(statusLetter(f("/a", "conflicted"))).toBe("U");
    expect(statusLetter(f("/a", "untracked", { from: "/b" }))).toBe("R");
    expect(statusLetter(f("/a", "deleted"))).toBe("D");
    expect(statusLetter(f("/a", "modified"))).toBe("M");
    expect(statusLetter(f("/a", "untracked", { staged: "added" }))).toBe("A");
    expect(statusLetter(f("/a", "untracked"))).toBe("?");
  });
});

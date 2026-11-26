// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { crumbs, normalizePath, parentOf, revOf, stateLetter } from "./model";

describe("file view model", () => {
  it("normalizes paths", () => {
    expect(normalizePath("a/b/")).toBe("/a/b");
    expect(normalizePath("//a///b")).toBe("/a/b");
    expect(normalizePath("")).toBe("/");
    expect(normalizePath("/")).toBe("/");
  });
  it("walks up and builds a breadcrumb", () => {
    expect(parentOf("/a/b/c.yml")).toBe("/a/b");
    expect(parentOf("/c.yml")).toBe("/");
    expect(crumbs("/a/b")).toEqual([
      { name: "a", path: "/a" },
      { name: "b", path: "/a/b" },
    ]);
    expect(crumbs("/")).toEqual([]);
  });
  it("reads the revision field", () => {
    expect(revOf("")).toBeNull();
    expect(revOf(" Working tree ")).toBeNull();
    expect(revOf(" v1.2 ")).toBe("v1.2");
  });
  it("maps states to tree letters", () => {
    expect(stateLetter("modified")).toBe("M");
    expect(stateLetter("untracked")).toBe("?");
    expect(stateLetter("deleted")).toBe("D");
    expect(stateLetter(undefined)).toBeNull();
  });
});

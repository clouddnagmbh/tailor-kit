// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitRef } from "../types";
import { defaultBase, localBranches, problem, publishState, pullWord, remoteBranches, titleFromBranch } from "./new-pull";

const ref = (name: string, kind: GitRef["kind"], over: Partial<GitRef> = {}): GitRef => ({
  full: kind === "remote" ? `refs/remotes/${name}` : `refs/heads/${name}`,
  name,
  kind,
  target: "abc",
  current: false,
  ahead: 0,
  behind: 0,
  ...over,
});

const refs: GitRef[] = [
  ref("main", "local"),
  ref("feature/currency", "local", { current: true, upstream: "origin/feature/currency", ahead: 2 }),
  ref("chore/new", "local"),
  ref("origin/main", "remote"),
  ref("origin/HEAD", "remote"),
  ref("origin/feature/currency", "remote"),
];

describe("create pull request model", () => {
  it("lists local branches, the current one first, and server branches without HEAD", () => {
    expect(localBranches(refs).map((r) => r.name)).toEqual(["feature/currency", "main", "chore/new"]);
    expect(remoteBranches(refs)).toEqual(["main", "feature/currency"]);
  });

  it("tells whether a branch is on the server", () => {
    expect(publishState(refs, "chore/new")).toBe("unpublished");
    expect(publishState(refs, "feature/currency")).toBe("ahead");
    expect(publishState(refs, "main")).toBe("published");
  });

  it("picks main before the other branches, never the head itself", () => {
    expect(defaultBase(["develop", "main"], [], "feature/x")).toBe("main");
    expect(defaultBase(["trunk", "feature/x"], [], "feature/x")).toBe("trunk");
    expect(defaultBase([], ["master", "x"], "x")).toBe("master");
    expect(defaultBase([], [], "x")).toBe("");
  });

  it("makes a title from the last part of a branch name", () => {
    expect(titleFromBranch("feature/add-currency")).toBe("Add currency");
    expect(titleFromBranch("fix_login_bug")).toBe("Fix login bug");
    expect(titleFromBranch("")).toBe("");
  });

  it("names what is missing, in the order the form is read", () => {
    const pull = { title: "T", body: "", head: "a", base: "b", draft: false };
    expect(problem({ ...pull, head: "" }, "published")).toMatch(/branch with your changes/);
    expect(problem({ ...pull, base: "" }, "published")).toMatch(/merge into/);
    expect(problem({ ...pull, base: "a" }, "published")).toMatch(/two different/);
    expect(problem(pull, "unpublished")).toMatch(/not on the server/);
    expect(problem({ ...pull, title: "  " }, "published")).toMatch(/title/);
    expect(problem(pull, "published")).toBeNull();
    expect(problem(pull, "ahead")).toBeNull();
  });

  it("says merge request for GitLab", () => {
    expect(pullWord({ kind: "gitlab", host: "h", api: "a", project: "p" })).toBe("merge request");
    expect(pullWord({ kind: "github", host: "h", api: "a", project: "p" })).toBe("pull request");
    expect(pullWord(null)).toBe("pull request");
  });
});

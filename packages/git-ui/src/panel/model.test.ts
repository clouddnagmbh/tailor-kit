// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitStatus, TreeNode } from "../types";
import {
  changeRows,
  commitLabel,
  initials,
  miniLanes,
  refPills,
  repoName,
  shortAgo,
  situation,
  syncAction,
  syncState,
  toolTiles,
} from "./model";

const status = (over: Partial<GitStatus> = {}): GitStatus => ({
  repo: true,
  branch: "main",
  upstream: "origin/main",
  ahead: 0,
  behind: 0,
  files: [],
  merging: false,
  ...over,
});

describe("situation", () => {
  it("names each state of the repository", () => {
    expect(situation(null, [], false, null).kind).toBe("loading");
    expect(situation(status({ repo: false }), [], false, null).kind).toBe("no-repo");
    expect(situation(status({ branch: null }), [{ name: "main", current: true, remote: false }], false, null)).toEqual({
      kind: "empty",
      branch: "main",
    });
    expect(situation(status({ merging: true, files: [{ path: "a.yml", state: "conflicted" }] }), [], true, 1)).toEqual({
      kind: "conflicts",
      files: 1,
    });
    expect(situation(status({ merging: true }), [], true, 1).kind).toBe("merging");
    expect(situation(status({ branch: null, upstream: null }), [], true, 1).kind).toBe("detached");
    expect(situation(status({ upstream: null }), [], true, 0).kind).toBe("no-remote");
    expect(situation(status({ upstream: null }), [], true, 1).kind).toBe("unpublished");
    expect(situation(status({ upstream: null }), [], true, null).kind).toBe("unpublished");
    expect(situation(status({ ahead: 1, behind: 2 }), [], true, 1).kind).toBe("diverged");
    expect(situation(status({ behind: 2 }), [], true, 1).kind).toBe("behind");
    expect(situation(status({ ahead: 1 }), [], true, 1).kind).toBe("ahead");
    expect(situation(status(), [], true, 1).kind).toBe("synced");
  });

  it("offers one contextual action, primary only when something waits", () => {
    const act = (s: GitStatus, remotes = 1) => syncAction(situation(s, [], true, remotes));
    expect(act(status({ behind: 2 }))).toMatchObject({ kind: "pull", label: "Pull 2", primary: true });
    expect(act(status({ ahead: 1 }))).toMatchObject({ kind: "push", label: "Push 1", primary: true });
    expect(act(status({ ahead: 1, behind: 1 }))).toMatchObject({ kind: "sync", primary: true });
    expect(act(status({ upstream: null }))).toMatchObject({ kind: "publish", label: "Publish branch" });
    expect(act(status({ upstream: null }), 0)).toMatchObject({ kind: "add-remote" });
    expect(act(status())).toMatchObject({ kind: "fetch", primary: false });
    expect(act(status({ branch: null }))).toMatchObject({ kind: "create-branch-here" });
  });

  it("says the sync state in short and in full", () => {
    const s = (st: GitStatus) => syncState(situation(st, [], true, 1));
    expect(s(status())).toMatchObject({ text: "Up to date", tone: "ok" });
    expect(s(status({ ahead: 1, behind: 2 }))).toMatchObject({ text: "↑1 ↓2", term: "diverged", tone: "warn" });
    expect(s(status({ ahead: 3 }))!.title).toBe("3 commits to send to origin/main.");
    expect(s(status({ upstream: null }))!.text).toBe("Not published");
  });
});

describe("changes", () => {
  const tree: TreeNode = {
    kind: "folder",
    name: "Demo",
    path: "",
    children: [
      { kind: "request", name: "Health", path: "health.yml", method: "GET" },
      { kind: "folder", name: "users", path: "users", children: [{ kind: "request", name: "List users", path: "users/list-users.yml", method: "POST" }] },
    ],
  };

  it("shows requests by name and method, other files by file name, with a badge", () => {
    const rows = changeRows(
      [
        { path: "health.yml", state: "modified" },
        { path: "users/list-users.yml", state: "untracked" },
        { path: "docs/readme.md", state: "deleted" },
      ],
      tree,
    );
    expect(rows.map((r) => [r.badge, r.method, r.title, r.dir])).toEqual([
      ["M", "GET", "Health", ""],
      ["A", "POST", "List users", "users"],
      ["D", null, "readme.md", "docs"],
    ]);
    expect(commitLabel(3)).toBe("Commit 3 changes");
    expect(commitLabel(1)).toBe("Commit 1 change");
  });
});

describe("tools", () => {
  it("makes short tiles with the full text as tip, graph first, conflicts and diffs elsewhere", () => {
    const cmd = (id: string, label: string) => ({ id, label, run: () => undefined });
    const tiles = toolTiles(
      [
        cmd("diff-working", "Changes: saved changes by field"),
        cmd("contract:open", "API contract: check requests against the OpenAPI spec"),
        cmd("git:compare", "Compare revisions…"),
        cmd("merge-conflicts", "Resolve merge conflicts"),
        cmd("new:thing", "Something new: with a long label"),
        cmd("git-history-lost", "Git: Lost work (reflog)"),
        cmd("git:other", "Git: Other thing"),
      ],
      () => undefined,
    );
    expect(tiles.map((t) => t.label)).toEqual(["Graph", "Compare", "Contract", "Lost work", "Something new", "Other thing"]);
    for (const t of tiles) {
      expect(t.label.length).toBeLessThanOrEqual(14);
      expect(t.tip.length).toBeGreaterThan(t.label.length);
    }
  });
});

describe("recent commits", () => {
  it("lays out lanes for a branch and its merge", () => {
    // m merges f into b; f and b come from a.
    const rows = miniLanes([
      { oid: "m", parents: ["b", "f"] },
      { oid: "f", parents: ["a"] },
      { oid: "b", parents: ["a"] },
      { oid: "a", parents: [] },
    ]);
    expect(rows.map((r) => r.lane)).toEqual([0, 1, 0, 0]);
    expect(rows[0]).toMatchObject({ merge: true, joins: [1], below: [0, 1] });
    expect(rows[1]).toMatchObject({ above: [0, 1], below: [0, 1] });
    expect(rows[3]).toMatchObject({ joins: [1], below: [] });
  });

  it("keeps a linear history in one lane, running off the bottom", () => {
    const rows = miniLanes([
      { oid: "c", parents: ["b"] },
      { oid: "b", parents: ["a"] },
    ]);
    expect(rows.map((r) => [r.lane, r.above, r.below])).toEqual([
      [0, [], [0]],
      [0, [0], [0]],
    ]);
  });

  it("labels commits with their refs, current branch first", () => {
    const refs = [
      { full: "refs/remotes/origin/main", name: "origin/main", kind: "remote" as const, target: "x", current: false, ahead: 0, behind: 0 },
      { full: "refs/tags/v1", name: "v1", kind: "tag" as const, target: "x", current: false, ahead: 0, behind: 0 },
      { full: "refs/heads/main", name: "main", kind: "local" as const, target: "x", current: true, ahead: 0, behind: 0 },
      { full: "refs/heads/other", name: "other", kind: "local" as const, target: "y", current: false, ahead: 0, behind: 0 },
    ];
    expect(refPills("x", refs).map((p) => p.name)).toEqual(["main", "origin/main", "v1"]);
  });

  it("shows initials, short times and the repository name", () => {
    expect(initials("Ada Lovelace")).toBe("AL");
    expect(initials("grace")).toBe("GR");
    expect(initials("")).toBe("?");
    const now = 1_700_000_000_000;
    expect(shortAgo(now / 1000 - 30, now)).toBe("now");
    expect(shortAgo(now / 1000 - 300, now)).toBe("5m");
    expect(shortAgo(now / 1000 - 3 * 3600, now)).toBe("3h");
    expect(shortAgo(now / 1000 - 2 * 86400, now)).toBe("2d");
    expect(repoName("https://git.example.com/acme/demo-api.git", "/x/y")).toBe("demo-api");
    expect(repoName("git@github.com:acme/api.git", null)).toBe("api");
    expect(repoName(null, "/Users/me/projects/payments")).toBe("payments");
    expect(repoName(null, null)).toBe("Repository");
  });
});

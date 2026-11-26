// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitStatus } from "./types";
import { ago, branchLabel, hunkHeader, mergeMessage, pullMessage, pushMessage, syncLabel } from "./format";

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

describe("Git view texts", () => {
  it("describes pull results", () => {
    expect(pullMessage({ outcome: "up-to-date" }, null)).toBe("Already up to date.");
    expect(pullMessage({ outcome: "fast-forward", commits: 1 }, null)).toBe("Pulled 1 commit (fast-forward).");
    expect(pullMessage({ outcome: "fast-forward", commits: 3 }, null)).toBe("Pulled 3 commits (fast-forward).");
    expect(pullMessage({ outcome: "merged", oid: "x" }, "origin/main")).toBe("Merged origin/main.");
    expect(pullMessage({ outcome: "conflicts", paths: ["a", "b"] }, null)).toMatch(/^Conflicts in 2 files/);
  });

  it("describes branch and sync state", () => {
    expect(syncLabel(status())).toBe("in sync with origin/main");
    expect(syncLabel(status({ ahead: 2, behind: 1 }))).toBe("↑2 ↓1 origin/main");
    expect(syncLabel(status({ upstream: null }))).toBe("not pushed yet");
    expect(branchLabel(status(), [])).toBe("main");
    expect(branchLabel(status({ branch: null }), [{ name: "main", current: true, remote: false }])).toBe(
      "main (no commits yet)",
    );
    expect(branchLabel(status({ branch: null }), [])).toBe("detached HEAD");
    expect(pushMessage(status({ upstream: null, branch: "x" }))).toBe("Pushed x to origin/x.");
    expect(mergeMessage(status())).toBe("Merge origin/main into main");
  });

  it("formats hunk headers and commit times", () => {
    expect(hunkHeader({ old_start: 2, old_lines: 7, new_start: 2, new_lines: 8, lines: [] })).toBe("@@ -2,7 +2,8 @@");
    const now = 1_800_000_000_000;
    expect(ago(now / 1000 - 30, now)).toBe("just now");
    expect(ago(now / 1000 - 600, now)).toBe("10 min ago");
    expect(ago(now / 1000 - 7200, now)).toBe("2 h ago");
    expect(ago(0, now)).toBe("1970-01-01");
  });
});

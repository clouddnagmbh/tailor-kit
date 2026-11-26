// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { ReviewComment, ReviewRepo } from "../types";
import { deviceLoginOffered, groupComments, relativeTime } from "./model";

const comment = (over: Partial<ReviewComment>): ReviewComment => ({
  id: "1",
  author: "grace",
  body: "x",
  created: 10,
  path: null,
  line: null,
  outdated: false,
  ...over,
});

describe("review model", () => {
  it("groups comments by file, general ones first, oldest first", () => {
    const groups = groupComments([
      comment({ id: "late", created: 30, path: "a.yml", line: 1 }),
      comment({ id: "general", created: 20 }),
      comment({ id: "early", created: 10, path: "a.yml", line: 2 }),
      comment({ id: "other", created: 15, path: "b.yml", line: 1 }),
    ]);
    expect(groups.map((g) => [g.file, g.comments.map((c) => c.id)])).toEqual([
      [null, ["general"]],
      ["a.yml", ["early", "late"]],
      ["b.yml", ["other"]],
    ]);
  });

  it("offers the device login only for the public hosts and only once the apps are registered", () => {
    const repo = (host: string): ReviewRepo => ({ kind: "github", host, api: "https://x", project: "a/b" });
    expect(deviceLoginOffered(repo("github.com"))).toBe(false);
    expect(deviceLoginOffered(repo("github.com"), true)).toBe(true);
    expect(deviceLoginOffered(repo("git.corp.example"), true)).toBe(false);
  });

  it("says how long ago", () => {
    expect(relativeTime(1000, 1030)).toBe("just now");
    expect(relativeTime(1000, 1000 + 180)).toBe("3 minutes ago");
    expect(relativeTime(1000, 1000 + 3600)).toBe("1 hour ago");
    expect(relativeTime(1000, 1000 + 2 * 86_400)).toBe("2 days ago");
  });
});

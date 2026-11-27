// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it, vi } from "vitest";

import type { GitExtension, GraphDropTarget, ProvidedGitExtensions } from "../extensions";
import { dropEntries, dropKey, dropPosition, graphDropActions } from "./drops";

const drop: GraphDropTarget = {
  repository: "/r",
  source: { kind: "ref", ref: "refs/heads/feature", commit: "b".repeat(40) },
  commit: "a".repeat(40),
  refs: ["refs/heads/main"],
  ref: "refs/heads/main",
  position: "above",
};

describe("graph drops (GitTailor wave 3, strand A)", () => {
  it("collects the actions of every extension in order and skips one that fails", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const one: GitExtension = {
      id: "one",
      graphDrops: (_ctx, d) => (d.ref === "refs/heads/main" ? [{ id: "merge", label: "Merge feature into main", run: () => {} }] : []),
    };
    const broken: GitExtension = {
      id: "broken",
      graphDrops: () => {
        throw new Error("no");
      },
    };
    const two: GitExtension = { id: "two", graphDrops: () => [{ id: "rebase", label: "Rebase feature onto main", run: () => {} }] };
    const provided: ProvidedGitExtensions = { extensions: [one, broken, two, { id: "none" }], ctx: {} };
    expect(graphDropActions(provided, drop).map((r) => r.key)).toEqual(["one/merge", "two/rebase"]);
    expect(graphDropActions(null, drop)).toEqual([]);
    expect(warn).toHaveBeenCalledOnce();
    warn.mockRestore();
  });

  it("keys a drop by what, where and which half", () => {
    expect(dropKey(drop)).not.toBe(dropKey({ ...drop, position: "below" }));
    expect(dropKey(drop)).not.toBe(dropKey({ ...drop, ref: undefined }));
    expect(dropKey({ ...drop, source: { kind: "commit", commit: "c" } })).toMatch(/^c:c>/);
  });

  it("tells the upper from the lower half of a row", () => {
    expect(dropPosition(105, 100, 28)).toBe("above");
    expect(dropPosition(120, 100, 28)).toBe("below");
  });

  it("runs a chosen action and reports its failure", async () => {
    const errors: string[] = [];
    const entries = dropEntries(
      [
        { key: "x/ok", extension: "x", action: { id: "ok", label: "OK", run: () => {} } },
        { key: "x/bad", extension: "x", action: { id: "bad", label: "Bad", run: () => Promise.reject(new Error("refused")) } },
      ],
      (m) => errors.push(m),
    );
    expect(entries.map((e) => ("label" in e ? e.label : ""))).toEqual(["OK", "Bad"]);
    for (const e of entries) if ("run" in e) e.run?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(errors).toEqual(["refused"]);
  });
});

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { describeLine, layoutBranchMap, pickTrunk, syntheticBranchMap, type MapCommit } from "./branchmap";

const DAY = 86_400;
const NOW = 1_800_000_000;
const c = (oid: string, parents: string[], day: number): MapCommit => ({ oid, parents, time: NOW - day * DAY });

describe("branch map layout", () => {
  //  m1 - m2 - m3 - m5(merge) - m6          main
  //        \       /
  //         f1 - f2                        feature
  //   m2 - g1                              fix (ahead of main's m2)
  const commits = [c("m1", [], 30), c("m2", ["m1"], 20), c("m3", ["m2"], 18), c("f1", ["m2"], 17), c("f2", ["f1"], 16), c("m5", ["m3", "f2"], 10), c("m6", ["m5"], 5), c("g1", ["m2"], 2)];
  const refs = [
    { name: "main", target: "m6", current: true },
    { name: "feature", target: "f2", ahead: 2 },
    { name: "fix", target: "g1", behind: 1 },
  ];
  const layout = layoutBranchMap({ commits, refs, now: NOW });

  it("puts the default branch in row 0 and every other branch in a row of its own where they overlap", () => {
    const [main, feature, fix] = ["main", "feature", "fix"].map((n) => layout.lines.find((l) => l.name === n)!);
    expect(layout.trunk).toBe("main");
    expect(main!.row).toBe(0);
    expect(feature!.row).not.toBe(0);
    expect(fix!.row).not.toBe(0);
    expect(fix!.row).not.toBe(feature!.row);
  });

  it("owns each commit once: the branches count their own commits", () => {
    const own = Object.fromEntries(layout.lines.map((l) => [l.name, l.commits]));
    expect(own).toEqual({ main: 5, feature: 2, fix: 1 });
  });

  it("finds the fork points and the merge", () => {
    const feature = layout.lines.find((l) => l.name === "feature")!;
    expect(feature.fork).toEqual({ oid: "m2", line: "main" });
    const merge = layout.links.find((l) => l.kind === "merge")!;
    expect(merge).toMatchObject({ fromLine: "feature", toLine: "main" });
    expect(layout.links.filter((l) => l.kind === "fork").map((l) => l.toLine).sort()).toEqual(["feature", "fix"]);
  });

  it("orders stations by commit time, evenly spaced", () => {
    const xs = layout.stations.map((s) => s.x);
    expect(xs).toEqual(xs.map((_, i) => i));
    const times = layout.stations.map((s) => commits.find((k) => k.oid === s.oid)!.time);
    expect([...times].sort((a, b) => a - b)).toEqual(times);
  });

  it("carries ahead, behind and age to the lines, and dims what is old", () => {
    const old = layoutBranchMap({ commits, refs, now: NOW + 200 * DAY });
    expect(layout.lines.find((l) => l.name === "feature")).toMatchObject({ ahead: 2, age: 16, dim: false });
    expect(old.lines.find((l) => l.name === "fix")!.dim).toBe(true);
    expect(old.lines.find((l) => l.name === "main")!.dim).toBe(false);
  });

  it("describes a line in words", () => {
    const f = layout.lines.find((l) => l.name === "feature")!;
    expect(describeLine(f, layout.trunk)).toBe("feature has 2 commits of its own, forked from main, 2 to push, last commit 16 days ago.");
  });

  it("is deterministic and survives refs that point at an unknown commit", () => {
    expect(layoutBranchMap({ commits, refs, now: NOW })).toEqual(layout);
    const odd = layoutBranchMap({ commits, refs: [...refs, { name: "ghost", target: "nope" }], now: NOW });
    expect(odd.lines.find((l) => l.name === "ghost")!.commits).toBe(0);
  });

  it("picks main, master, trunk, else the current branch", () => {
    expect(pickTrunk([{ name: "dev", target: "x", current: true }, { name: "master", target: "y" }])!.name).toBe("master");
    expect(pickTrunk([{ name: "dev", target: "x" }, { name: "wip", target: "y", current: true }])!.name).toBe("wip");
    expect(pickTrunk([], "main")).toBeUndefined();
    expect(layoutBranchMap({ commits: [], refs: [], now: NOW }).lines).toEqual([]);
  });

  it("stays quick and packed with 500 branches", () => {
    const big = syntheticBranchMap(500);
    const t0 = performance.now();
    const l = layoutBranchMap(big);
    const ms = performance.now() - t0;
    expect(l.lines).toHaveLength(501);
    expect(ms).toBeLessThan(200);
    // Lines that share a row never overlap.
    const byRow = new Map<number, typeof l.lines>();
    for (const line of l.lines) byRow.set(line.row, [...(byRow.get(line.row) ?? []), line]);
    for (const list of byRow.values()) {
      const sorted = [...list].sort((a, b) => a.from - b.from);
      // The label after the tip counts: no line starts before the previous one's label ended.
      for (let i = 1; i < sorted.length; i++) expect(sorted[i]!.from).toBeGreaterThan(sorted[i - 1]!.to);
    }
    // Packed: far fewer rows than branches.
    expect(l.maxRow - l.minRow).toBeLessThan(120);
  });
});

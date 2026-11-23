// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { LaneLayout, layoutLanes, type LaneRow } from "./layout";
import { syntheticHistory } from "./synthetic";

const c = (oid: string, ...parents: string[]) => ({ oid, parents });

/** Every lane that ends at the bottom of row i starts at the top of row i + 1. */
function expectContinuous(rows: LaneRow[]) {
  for (let i = 0; i + 1 < rows.length; i++) {
    const below = new Set([...rows[i].through.map((t) => t.lane), ...rows[i].down.map((d) => d.to)]);
    const above = new Set([...rows[i + 1].through.map((t) => t.lane), ...rows[i + 1].up.map((u) => u.from)]);
    expect([...above].sort(), `rows ${i} → ${i + 1}`).toEqual([...below].sort());
  }
}

describe("lane layout", () => {
  it("keeps a straight history in one lane", () => {
    const rows = layoutLanes([c("c", "b"), c("b", "a"), c("a")]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(rows[0].up).toEqual([]);
    expect(rows[1].up).toEqual([{ from: 0, color: rows[0].color }]);
    expect(rows[2].down).toEqual([]);
    expect(new Set(rows.map((r) => r.color)).size).toBe(1);
    expectContinuous(rows);
  });

  it("joins a fork at the common parent", () => {
    // main: m2 → base; feature: f1 → base. Both tips first.
    const rows = layoutLanes([c("m2", "base"), c("f1", "base"), c("base")]);
    expect(rows[0].lane).toBe(0);
    expect(rows[1].lane).toBe(1);
    expect(rows[1].through).toEqual([{ lane: 0, color: rows[0].color }]);
    expect(rows[1].color).not.toBe(rows[0].color);
    // The parent takes the leftmost lane; the other one curves in.
    expect(rows[2].lane).toBe(0);
    expect(rows[2].up.map((u) => u.from).sort()).toEqual([0, 1]);
    expectContinuous(rows);
  });

  it("opens a lane for the second parent of a merge", () => {
    const rows = layoutLanes([c("merge", "m1", "f1"), c("f1", "base"), c("m1", "base"), c("base")]);
    expect(rows[0].down).toEqual([
      { to: 0, color: rows[0].color },
      { to: 1, color: rows[1].color },
    ]);
    expect(rows[0].merge).toBe(true);
    expect(rows[1].lane).toBe(1);
    expect(rows[2].lane).toBe(0);
    expect(rows[3].up.map((u) => u.from).sort()).toEqual([0, 1]);
    expect(rows.map((r) => r.width)).toEqual([2, 2, 2, 2]);
    expectContinuous(rows);
  });

  it("merges into a lane that already waits for the parent", () => {
    // Two merges of the same feature commit: the second edge joins the existing lane.
    const rows = layoutLanes([c("m2", "m1", "f"), c("m1", "base", "f"), c("f", "base"), c("base")]);
    expect(rows[1].down.map((d) => d.to)).toEqual([0, 1]);
    // The feature's lane goes on; the second merge edge curves into it.
    expect(rows[1].through.map((t) => t.lane)).toEqual([1]);
    expect(rows[2].lane).toBe(1);
    expectContinuous(rows);
  });

  it("draws an octopus merge with one lane per parent", () => {
    const rows = layoutLanes([c("o", "a", "b", "d"), c("a", "r"), c("b", "r"), c("d", "r"), c("r")]);
    expect(rows[0].down.map((d) => d.to)).toEqual([0, 1, 2]);
    expect(rows.slice(1, 4).map((r) => r.lane)).toEqual([0, 1, 2]);
    expect(rows[4].up.map((u) => u.from).sort()).toEqual([0, 1, 2]);
    expect(new Set(rows[0].down.map((d) => d.color)).size).toBe(3);
    expectContinuous(rows);
  });

  it("gives every root its own line and frees it", () => {
    const rows = layoutLanes([c("a2", "a1"), c("b1"), c("a1")]);
    expect(rows[1].lane).toBe(1);
    expect(rows[1].down).toEqual([]);
    expect(rows[2].lane).toBe(0);
    expect(rows[2].through).toEqual([]);
    expect(rows[2].width).toBe(1);
    expectContinuous(rows);
  });

  it("reuses lanes once their line ended", () => {
    // A side branch ends (merged), then a new tip appears: it takes the freed lane.
    const rows = layoutLanes([
      c("m", "x", "s"),
      c("s", "x"),
      c("x", "y"),
      c("t", "y"),
      c("y"),
    ]);
    expect(rows[1].lane).toBe(1);
    expect(rows[2].up.map((u) => u.from).sort()).toEqual([0, 1]);
    expect(rows[3].lane).toBe(1);
    expect(Math.max(...rows.map((r) => r.width))).toBe(2);
    expectContinuous(rows);
  });

  it("continues across pages exactly as in one go", () => {
    const history = syntheticHistory(600, 7);
    const whole = layoutLanes(history);
    const paged = new LaneLayout();
    const rows = [...paged.add(history.slice(0, 250)), ...paged.add(history.slice(250))];
    expect(rows).toEqual(whole);
    expect(paged.maxWidth).toBe(Math.max(...whole.map((r) => r.width)));
    expectContinuous(whole);
  });

  it("lays out 10 000 commits in under 100 ms", () => {
    const history = syntheticHistory(10_000, 1);
    layoutLanes(history.slice(0, 500)); // warm up the JIT
    const started = performance.now();
    const rows = layoutLanes(history);
    const ms = performance.now() - started;
    expect(rows).toHaveLength(10_000);
    expect(ms).toBeLessThan(100);
    expectContinuous(rows.slice(0, 2_000));
  });
});

describe("synthetic history", () => {
  it("is topological, deterministic and has merges, branches and two roots", () => {
    const a = syntheticHistory(2_000, 3);
    expect(syntheticHistory(2_000, 3)).toEqual(a);
    const seen = new Set<string>();
    for (const commit of [...a].reverse()) {
      for (const p of commit.parents) expect(seen.has(p), `${commit.oid} after its parent ${p}`).toBe(true);
      seen.add(commit.oid);
    }
    expect(a.some((x) => x.parents.length > 1)).toBe(true);
    expect(a.filter((x) => x.parents.length === 0).length).toBe(2);
  });
});

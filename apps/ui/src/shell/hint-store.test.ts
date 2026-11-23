// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { HintStore } from "./hint-store";

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
    data,
  };
}

describe("hint store (wave 14)", () => {
  beforeEach(() => vi.stubGlobal("localStorage", memoryStorage()));
  afterEach(() => vi.unstubAllGlobals());

  it("remembers a dismissal across a reload", () => {
    const a = new HintStore();
    expect(a.has("x/tip")).toBe(false);
    a.dismiss("x/tip");
    expect(a.has("x/tip")).toBe(true);
    const reloaded = new HintStore();
    expect(reloaded.has("x/tip")).toBe(true);
    expect(reloaded.has("x/other")).toBe(false);
  });

  it("tells subscribers about a dismissal", () => {
    const store = new HintStore();
    const seen: number[] = [];
    store.dismissed.subscribe((s) => seen.push(s.size));
    store.dismiss("a");
    store.dismiss("b");
    expect(seen).toEqual([0, 1, 2]);
  });

  it("ignores rubbish in the storage", () => {
    for (const raw of ["{not json", '{"a":1}', "[1,2]", "null"]) {
      vi.stubGlobal("localStorage", memoryStorage({ "rt.shell.hints.dismissed": raw }));
      const store = new HintStore();
      expect(store.has("a")).toBe(false);
      store.dismiss("a");
      expect(store.has("a")).toBe(true);
    }
  });

  it("works when the storage throws or is missing", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("full");
      },
      removeItem: () => {},
    };
    vi.stubGlobal("localStorage", broken);
    const store = new HintStore();
    expect(() => store.dismiss("a")).not.toThrow();
    expect(store.has("a")).toBe(true);
    vi.stubGlobal("localStorage", undefined);
    expect(() => new HintStore().dismiss("b")).not.toThrow();
  });
});

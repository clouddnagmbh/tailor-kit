// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { afterEach, describe, expect, it, vi } from "vitest";

import { createShellStorage, isNumber } from "./storage";

describe("shell storage", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("keys every value with the product's prefix", () => {
    const data = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v), removeItem: (k: string) => void data.delete(k) });
    const s = createShellStorage("x.shell.");
    s.store("width", 240);
    expect([...data.keys()]).toEqual(["x.shell.width"]);
    expect(s.load("width", 0, isNumber)).toBe(240);
    expect(s.load("width", 0, (v): v is number => v === "no")).toBe(0);
    s.store("width", undefined);
    expect(data.size).toBe(0);
  });

  it("falls back when the storage is missing", () => {
    vi.stubGlobal("localStorage", undefined);
    const s = createShellStorage("x.");
    expect(() => s.store("a", 1)).not.toThrow();
    expect(s.load("a", 7, isNumber)).toBe(7);
  });
});

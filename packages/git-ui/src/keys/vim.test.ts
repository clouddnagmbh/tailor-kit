// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { createVimKeymap, inTextField, nextMatch, type VimKeyEvent, type VimList } from "./vim";

const key = (k: string, extra: Partial<VimKeyEvent> = {}): VimKeyEvent => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, target: null, ...extra });

/** A fake list of `labels` that records what the keymap did. */
function fake(labels: string[], extra: Partial<VimList> = {}) {
  const state = { at: 0, activated: 0, opened: [] as boolean[] };
  const list: VimList = {
    count: () => labels.length,
    index: () => state.at,
    select: (i) => (state.at = i),
    page: () => 10,
    labels: () => labels,
    ...extra,
  };
  return { state, list };
}
const names = Array.from({ length: 30 }, (_, i) => `file-${i}`);

describe("vim keymap (generic list)", () => {
  it("j and k move, clamped to the ends", () => {
    const { state, list } = fake(names);
    const km = createVimKeymap(() => list);
    expect(km.handle(key("j"))).toBe(true);
    expect(state.at).toBe(1);
    km.handle(key("k"));
    km.handle(key("k"));
    expect(state.at).toBe(0);
    state.at = 29;
    km.handle(key("j"));
    expect(state.at).toBe(29);
  });

  it("gg goes to the first item, G to the last; a single g waits", () => {
    const { state, list } = fake(names);
    const km = createVimKeymap(() => list);
    state.at = 12;
    km.handle(key("g"));
    expect(state.at).toBe(12);
    km.handle(key("g"));
    expect(state.at).toBe(0);
    km.handle(key("G"));
    expect(state.at).toBe(29);
    state.at = 5;
    km.handle(key("g"));
    km.handle(key("j"));
    km.handle(key("g"));
    expect(state.at).toBe(6);
  });

  it("Ctrl+d and Ctrl+u move half a page", () => {
    const { state, list } = fake(names);
    const km = createVimKeymap(() => list);
    km.handle(key("d", { ctrlKey: true }));
    expect(state.at).toBe(5);
    km.handle(key("u", { ctrlKey: true }));
    km.handle(key("u", { ctrlKey: true }));
    expect(state.at).toBe(0);
    expect(km.handle(key("x", { ctrlKey: true }))).toBe(false);
  });

  it("is a no-op when the list is off (null)", () => {
    const { state } = fake(names);
    const km = createVimKeymap(() => null);
    for (const k of ["j", "k", "g", "G", "/", "n"]) expect(km.handle(key(k))).toBe(false);
    expect(km.handle(key("d", { ctrlKey: true }))).toBe(false);
    expect(state.at).toBe(0);
  });

  it("does nothing in text fields", () => {
    const { state, list } = fake(names);
    const km = createVimKeymap(() => list);
    const field = { closest: (s: string) => (s.includes("textarea") ? {} : null), isContentEditable: false };
    expect(km.handle(key("j", { target: field }))).toBe(false);
    expect(state.at).toBe(0);
    expect(inTextField({ closest: () => null, isContentEditable: true })).toBe(true);
    expect(inTextField(null)).toBe(false);
  });

  it("ignores Meta and Alt chords", () => {
    const { list } = fake(names);
    const km = createVimKeymap(() => list);
    expect(km.handle(key("j", { metaKey: true }))).toBe(false);
    expect(km.handle(key("j", { altKey: true }))).toBe(false);
  });

  it("empty lists do not throw", () => {
    const { list } = fake([]);
    const km = createVimKeymap(() => list);
    expect(() => ["j", "k", "G", "n"].forEach((k) => km.handle(key(k)))).not.toThrow();
  });

  it("/ opens the inline find, typing selects the first match, Enter keeps it, n and N cycle", () => {
    const { state, list } = fake(["alpha", "beta", "gamma", "beta2"]);
    const shown: (string | null)[] = [];
    const km = createVimKeymap(() => list, (t) => shown.push(t));
    km.handle(key("/"));
    expect(km.finding).toBe("");
    for (const c of "bet") km.handle(key(c));
    expect(state.at).toBe(1);
    km.handle(key("Enter"));
    expect(km.finding).toBeNull();
    expect(shown.at(-1)).toBeNull();
    km.handle(key("n"));
    expect(state.at).toBe(3);
    km.handle(key("n"));
    expect(state.at).toBe(1);
    km.handle(key("N"));
    expect(state.at).toBe(3);
  });

  it("Esc leaves the find without searching; Backspace edits it", () => {
    const { state, list } = fake(["alpha", "beta"]);
    const km = createVimKeymap(() => list);
    km.handle(key("/"));
    km.handle(key("b"));
    km.handle(key("z"));
    km.handle(key("Backspace"));
    expect(km.finding).toBe("b");
    km.handle(key("Escape"));
    expect(km.finding).toBeNull();
    km.handle(key("n"));
    expect(state.at).toBe(1);
  });

  it("the list's own find and repeat win over the inline find", () => {
    let found = 0;
    const { list } = fake(names, { find: () => (found++, true), repeat: () => true });
    const km = createVimKeymap(() => list);
    expect(km.handle(key("/"))).toBe(true);
    expect(found).toBe(1);
    expect(km.finding).toBeNull();
  });

  it("Enter and h/l are left alone when the list does not offer them", () => {
    const { list } = fake(names);
    const km = createVimKeymap(() => list);
    expect(km.handle(key("Enter"))).toBe(false);
    expect(km.handle(key("h"))).toBe(false);
    expect(km.handle(key("l"))).toBe(false);
  });
});

describe("vim keymap (per list)", () => {
  it("graph: selects by index and repeats the search hits", () => {
    const hits = [3, 7];
    let at = 0;
    let hit = -1;
    const list: VimList = {
      count: () => 100,
      index: () => at,
      select: (i) => (at = i),
      page: () => 20,
      find: () => true,
      repeat: (dir) => {
        hit = (hit + dir + hits.length) % hits.length;
        at = hits[hit]!;
        return true;
      },
    };
    const km = createVimKeymap(() => list);
    km.handle(key("d", { ctrlKey: true }));
    expect(at).toBe(10);
    km.handle(key("n"));
    expect(at).toBe(3);
    km.handle(key("n"));
    expect(at).toBe(7);
    km.handle(key("N"));
    expect(at).toBe(3);
  });

  it("changes: Enter activates the selected file", () => {
    const opened: number[] = [];
    let at = 0;
    const list: VimList = { count: () => 4, index: () => at, select: (i) => (at = i), activate: () => void opened.push(at) };
    const km = createVimKeymap(() => list);
    km.handle(key("j"));
    km.handle(key("j"));
    expect(km.handle(key("Enter"))).toBe(true);
    expect(opened).toEqual([2]);
  });

  it("file tree: l expands, h collapses", () => {
    const calls: boolean[] = [];
    const list: VimList = { count: () => 3, index: () => 0, select: () => {}, open: (e) => void calls.push(e) };
    const km = createVimKeymap(() => list);
    km.handle(key("l"));
    km.handle(key("h"));
    expect(calls).toEqual([true, false]);
  });

  it("file history: find over summaries", () => {
    expect(nextMatch(["fix a", "add b", "fix c"], "FIX", 0, 1)).toBe(2);
    expect(nextMatch(["fix a"], "zzz", 0, 1)).toBe(-1);
    expect(nextMatch(["a", "b"], "a", 0, -1)).toBe(0);
  });
});

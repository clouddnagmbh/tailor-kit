// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { menuKey, typeahead, type MenuEntry } from "./menu";
import { rove, step } from "./roving";

const entries: MenuEntry[] = [
  { heading: "Branch" },
  { id: "new", label: "New branch…" },
  { id: "rename", label: "Rename…", disabled: true },
  { separator: true },
  { id: "merge", label: "Merge into current", items: [{ label: "main" }] },
  { id: "delete", label: "Delete", danger: true },
];

describe("menu keyboard model", () => {
  it("moves over enabled items only, wrapping at both ends", () => {
    expect(menuKey(entries, -1, "ArrowDown")).toEqual({ type: "focus", index: 1 });
    expect(menuKey(entries, 1, "ArrowDown")).toEqual({ type: "focus", index: 4 });
    expect(menuKey(entries, 5, "ArrowDown")).toEqual({ type: "focus", index: 1 });
    expect(menuKey(entries, 1, "ArrowUp")).toEqual({ type: "focus", index: 5 });
    expect(menuKey(entries, -1, "ArrowUp")).toEqual({ type: "focus", index: 5 });
  });

  it("jumps with Home, End and the first letter", () => {
    expect(menuKey(entries, 4, "Home")).toEqual({ type: "focus", index: 1 });
    expect(menuKey(entries, 1, "End")).toEqual({ type: "focus", index: 5 });
    expect(menuKey(entries, 1, "d")).toEqual({ type: "focus", index: 5 });
    expect(menuKey(entries, 1, "R")).toEqual({ type: "focus", index: 1 }); // "Rename" is disabled
    expect(typeahead(entries, 5, "n")).toBe(1); // wraps
  });

  it("activates with Enter and Space, opens submenus with Enter and ArrowRight", () => {
    expect(menuKey(entries, 1, "Enter")).toEqual({ type: "activate", index: 1 });
    expect(menuKey(entries, 5, " ")).toEqual({ type: "activate", index: 5 });
    expect(menuKey(entries, 4, "Enter")).toEqual({ type: "open-submenu", index: 4 });
    expect(menuKey(entries, 4, "ArrowRight")).toEqual({ type: "open-submenu", index: 4 });
    expect(menuKey(entries, 1, "ArrowRight")).toEqual({ type: "none" });
    expect(menuKey(entries, 2, "Enter")).toEqual({ type: "none" }); // disabled
  });

  it("closes: Escape restores focus, Tab does not; a submenu closes itself with ArrowLeft or Escape", () => {
    expect(menuKey(entries, 1, "Escape")).toEqual({ type: "close", restoreFocus: true });
    expect(menuKey(entries, 1, "Tab")).toEqual({ type: "close", restoreFocus: false });
    expect(menuKey(entries, 1, "ArrowLeft")).toEqual({ type: "none" });
    expect(menuKey(entries, 0, "ArrowLeft", true)).toEqual({ type: "close-submenu" });
    expect(menuKey(entries, 0, "Escape", true)).toEqual({ type: "close-submenu" });
  });

  it("ignores other keys", () => {
    expect(menuKey(entries, 1, "Shift")).toEqual({ type: "none" });
    expect(menuKey(entries, 1, "F2")).toEqual({ type: "none" });
  });
});

describe("roving focus (tabs, segmented controls)", () => {
  const enabled = [true, false, true, true];
  it("steps over disabled entries and wraps", () => {
    expect(step(enabled, 0, 1)).toBe(2);
    expect(step(enabled, 3, 1)).toBe(0);
    expect(step(enabled, 0, -1)).toBe(3);
    expect(step([false, false], 0, 1)).toBe(0);
  });
  it("maps keys per orientation", () => {
    expect(rove(enabled, 0, "ArrowRight", "horizontal")).toBe(2);
    expect(rove(enabled, 0, "ArrowDown", "horizontal")).toBeNull();
    expect(rove(enabled, 0, "ArrowDown", "vertical")).toBe(2);
    expect(rove(enabled, 2, "Home", "horizontal")).toBe(0);
    expect(rove(enabled, 0, "End", "vertical")).toBe(3);
    expect(rove(enabled, 0, "Enter", "vertical")).toBeNull();
  });
});

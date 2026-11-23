// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { NEW_ICONS } from "../new/icons";
import { colourIndex, initials } from "./avatar";
import { ICON_NAMES, iconFor, ICONS, isIconName } from "./icons";
import { formatKeys } from "./keys";

describe("icons", () => {
  it("covers the New… set and the names the strands use", () => {
    for (const name of NEW_ICONS) expect(isIconName(name)).toBe(true);
    for (const name of ["branch", "commit", "merge", "pull", "push", "sync", "stash", "tag", "compare", "history"])
      expect(ICON_NAMES).toContain(name);
    for (const name of ["bisect", "review", "contract", "shield-secret", "graph", "add", "close", "more", "chevron"])
      expect(ICON_NAMES).toContain(name);
    for (const name of ["check", "warning", "info", "search", "settings", "undo", "external-link"])
      expect(ICON_NAMES).toContain(name);
  });

  it("falls back for unknown names from extensions", () => {
    expect(iconFor("no-such-icon")).toBe(ICONS.spark);
    expect(iconFor(undefined, "folder")).toBe(ICONS.folder);
    expect(iconFor("commit")).toBe(ICONS.commit);
    expect(isIconName("toString")).toBe(false);
  });
});

describe("avatar", () => {
  it("takes two initials from names and e-mail addresses", () => {
    expect(initials("Ada Lovelace")).toBe("AL");
    expect(initials("ada.lovelace@example.com")).toBe("AL");
    expect(initials("grace")).toBe("GR");
    expect(initials("Jean-Luc van der Berg")).toBe("JB");
    expect(initials("  ")).toBe("?");
  });

  it("picks a stable colour from 1 to 8", () => {
    expect(colourIndex("Ada Lovelace")).toBe(colourIndex("ada lovelace "));
    const all = new Set(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"].map((n) => colourIndex(n)));
    for (const i of all) expect(i >= 1 && i <= 8).toBe(true);
    expect(all.size).toBeGreaterThan(3);
  });
});

describe("shortcut display", () => {
  it("uses symbols on macOS and words elsewhere", () => {
    expect(formatKeys("Mod+Shift+K", true)).toEqual(["⌘", "⇧", "K"]);
    expect(formatKeys("Mod+Enter", false)).toEqual(["Ctrl", "↵"]);
    expect(formatKeys("Alt+ArrowUp", false)).toEqual(["Alt", "↑"]);
    expect(formatKeys("Escape", true)).toEqual(["Esc"]);
  });
});

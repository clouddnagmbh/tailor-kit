// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { formatShortcut, matchShortcut, type Shortcut } from "./shortcuts";

const save: Shortcut = { command: "save", key: "s" };
const next: Shortcut = { command: "next", key: "Tab", ctrl: true };
const digit: Shortcut = { command: "digit-1", key: "1", digits: true };
const table: Shortcut[] = [save, next, digit];
const aliases: Shortcut[] = [{ command: "next", key: "]", code: "BracketRight", shift: true, macOnly: true }];
const key = (k: string, mods: object = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

describe("shortcut matching", () => {
  it("matches Mod per platform, Ctrl itself and macOS-only aliases", () => {
    expect(matchShortcut(key("s", { metaKey: true }), true, table)?.command).toBe("save");
    expect(matchShortcut(key("s", { ctrlKey: true }), true, table)).toBeUndefined();
    expect(matchShortcut(key("Tab", { ctrlKey: true }), true, table)?.command).toBe("next");
    const brace = { ...key("}", { metaKey: true, shiftKey: true }), code: "BracketRight" };
    expect(matchShortcut(brace, true, table, aliases)?.command).toBe("next");
    expect(matchShortcut({ ...brace, metaKey: false, ctrlKey: true }, false, table, aliases)).toBeUndefined();
  });

  it("formats per platform", () => {
    expect(formatShortcut(save, true)).toBe("⌘S");
    expect(formatShortcut(next, false)).toBe("Ctrl+Tab");
    expect(formatShortcut(digit, false)).toBe("Ctrl+1…9");
  });
});

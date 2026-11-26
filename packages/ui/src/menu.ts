// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Menu entries and the keyboard model of Menu.svelte (WAI-ARIA menu pattern): arrows move and
// wrap over enabled items, Home/End jump, a letter jumps to the next item starting with it,
// Enter/Space activate, ArrowRight opens a submenu and ArrowLeft closes it, Escape closes and
// returns focus to the trigger, Tab closes. Pure, tested in menu.test.ts.

import { firstEnabled, lastEnabled, step } from "./roving";

export interface MenuItem {
  /** Stable id, handed back by `onselect`. */
  id?: string;
  label: string;
  /** A semantic icon name (ui/icons.ts). */
  icon?: string;
  /** A shortcut shown on the right, in `Kbd` notation (`Mod+Shift+K`). */
  shortcut?: string;
  /** Secondary text after the label. */
  hint?: string;
  /** Destructive: drawn in the danger colour. */
  danger?: boolean;
  disabled?: boolean;
  /** A check mark (a `menuitemcheckbox`); leave out for plain items. */
  checked?: boolean;
  /** A submenu. */
  items?: MenuEntry[];
  run?: () => unknown;
}

export interface MenuSeparator {
  separator: true;
}

export interface MenuHeading {
  heading: string;
}

export type MenuEntry = MenuItem | MenuSeparator | MenuHeading;

export const isItem = (e: MenuEntry): e is MenuItem => "label" in e;
export const isSeparator = (e: MenuEntry): e is MenuSeparator => "separator" in e;

/** Which entries can take focus: items that are not disabled. */
export const enabledMask = (entries: readonly MenuEntry[]): boolean[] => entries.map((e) => isItem(e) && !e.disabled);

export type MenuAction =
  | { type: "focus"; index: number }
  | { type: "activate"; index: number }
  | { type: "open-submenu"; index: number }
  | { type: "close-submenu" }
  | { type: "close"; restoreFocus: boolean }
  | { type: "none" };

/** The item after `current` whose label starts with `char` (case-insensitive), wrapping. */
export function typeahead(entries: readonly MenuEntry[], current: number, char: string): number {
  const c = char.toLocaleLowerCase();
  const n = entries.length;
  for (let i = 1; i <= n; i++) {
    const j = (current + i) % n;
    const e = entries[j];
    if (isItem(e) && !e.disabled && e.label.toLocaleLowerCase().startsWith(c)) return j;
  }
  return current;
}

/** What a key does in a menu whose focused entry is `current` (`-1`: none yet). */
export function menuKey(entries: readonly MenuEntry[], current: number, key: string, nested = false): MenuAction {
  const mask = enabledMask(entries);
  const entry = entries[current];
  const item = entry && isItem(entry) && !entry.disabled ? entry : undefined;
  switch (key) {
    case "ArrowDown":
      return { type: "focus", index: current < 0 ? firstEnabled(mask) : step(mask, current, 1) };
    case "ArrowUp":
      return { type: "focus", index: current < 0 ? lastEnabled(mask) : step(mask, current, -1) };
    case "Home":
      return { type: "focus", index: firstEnabled(mask) };
    case "End":
      return { type: "focus", index: lastEnabled(mask) };
    case "Enter":
    case " ":
      if (!item) return { type: "none" };
      return item.items?.length ? { type: "open-submenu", index: current } : { type: "activate", index: current };
    case "ArrowRight":
      return item?.items?.length ? { type: "open-submenu", index: current } : { type: "none" };
    case "ArrowLeft":
      return nested ? { type: "close-submenu" } : { type: "none" };
    case "Escape":
      return nested ? { type: "close-submenu" } : { type: "close", restoreFocus: true };
    case "Tab":
      return { type: "close", restoreFocus: false };
    default:
      if (key.length === 1 && key !== " ") return { type: "focus", index: typeahead(entries, current, key) };
      return { type: "none" };
  }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Display of shortcuts in `Kbd` notation ("Mod+Shift+K"): Mod is ⌘ on macOS and Ctrl elsewhere.

const MAC: Record<string, string> = { mod: "⌘", cmd: "⌘", ctrl: "⌃", alt: "⌥", option: "⌥", shift: "⇧" };
const OTHER: Record<string, string> = { mod: "Ctrl", cmd: "Ctrl", ctrl: "Ctrl", alt: "Alt", option: "Alt", shift: "Shift" };
const NAMED: Record<string, string> = {
  enter: "↵",
  return: "↵",
  escape: "Esc",
  esc: "Esc",
  backspace: "⌫",
  delete: "Del",
  tab: "Tab",
  space: "Space",
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
};

/** The keys of a shortcut as shown: `formatKeys("Mod+Enter", true)` → `["⌘", "↵"]`. */
export function formatKeys(shortcut: string, mac: boolean): string[] {
  return shortcut
    .split("+")
    .map((k) => k.trim())
    .filter(Boolean)
    .map((k) => {
      const l = k.toLowerCase();
      return (mac ? MAC : OTHER)[l] ?? NAMED[l] ?? (k.length === 1 ? k.toUpperCase() : k);
    });
}

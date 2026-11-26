// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// Matching and display of keyboard shortcuts. A product owns its table (`Shortcut<CommandId>[]`)
// and passes it in; this module knows only the rules: "Mod" is Cmd on macOS and Ctrl elsewhere,
// and every shortcut also fires while focus is in an input or a code editor (the product's
// handler listens in the capture phase, before the editor sees the key).

export interface Shortcut<C extends string = string> {
  command: C;
  /** `KeyboardEvent.key`, compared case-insensitively. Combined with Mod unless `shiftAlt`. */
  key: string;
  /** Shift+Alt instead of Mod (a letter key, matched by `KeyboardEvent.code` too: Alt changes `key` on macOS). */
  shiftAlt?: boolean;
  /** The Control key itself on every platform (not Cmd on macOS): Ctrl+Tab. */
  ctrl?: boolean;
  /** Shift is part of the shortcut (with Mod or Ctrl). */
  shift?: boolean;
  /** Matched by `KeyboardEvent.code` (layout independent) instead of `key`. */
  code?: string;
  /** Only on macOS (Cmd+Shift+] is the macOS convention; elsewhere those keys are typing). */
  macOnly?: boolean;
  /** Not listed on the start screen (the digit shortcuts show as one row). */
  unlisted?: boolean;
  /** Shown as the range "Mod+1…9" (the first of the digit shortcuts that stand for one row). */
  digits?: boolean;
}

export { isMacPlatform } from "@tailor-kit/ui";

type KeyLike = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"> & { code?: string };

/** The shortcut of `shortcuts` (or the unlisted `aliases`) this key event triggers, if any. */
export function matchShortcut<C extends string>(
  e: KeyLike,
  mac: boolean,
  shortcuts: readonly Shortcut<C>[],
  aliases: readonly Shortcut<C>[] = [],
): Shortcut<C> | undefined {
  const key = e.key.toLowerCase();
  if (e.shiftKey && e.altKey && !e.metaKey && !e.ctrlKey) {
    return shortcuts.find((s) => s.shiftAlt && (e.code === `Key${s.key.toUpperCase()}` || s.key.toLowerCase() === key));
  }
  const mod = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  const onlyCtrl = e.ctrlKey && !e.metaKey;
  if (e.altKey) return undefined;
  return [...shortcuts, ...aliases].find((s) => {
    if (s.shiftAlt || (s.macOnly && !mac)) return false;
    if (!(s.ctrl ? onlyCtrl : mod) || e.shiftKey !== !!s.shift) return false;
    return s.code ? e.code === s.code || key === s.key : s.key.toLowerCase() === key;
  });
}

/** Display form: "⌘S" / "⇧⌥F" on macOS, "Ctrl+S" / "Shift+Alt+F" elsewhere. */
export function formatShortcut(s: Shortcut, mac: boolean): string {
  if (s.digits) return mac ? `⌘${s.key}…9` : `Ctrl+${s.key}…9`;
  if (s.ctrl) return `${mac ? "⌃" : "Ctrl+"}${s.shift ? (mac ? "⇧" : "Shift+") : ""}${s.key}`;
  const key = s.key === "Enter" ? (mac ? "↩" : "Enter") : s.key.toUpperCase();
  if (s.shiftAlt) return mac ? `⇧⌥${key}` : `Shift+Alt+${key}`;
  return mac ? `⌘${key}` : `Ctrl+${key}`;
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Keyboard movement in a row or list of options (tabs, segmented controls, menus): arrows step
// and wrap, Home and End jump, disabled entries are skipped. Pure, tested in roving.test.ts.

export type Orientation = "horizontal" | "vertical";

/** The next enabled index from `current` in direction `delta`, wrapping; `current` if none. */
export function step(enabled: readonly boolean[], current: number, delta: 1 | -1): number {
  const n = enabled.length;
  for (let i = 1; i <= n; i++) {
    const j = (((current + delta * i) % n) + n) % n;
    if (enabled[j]) return j;
  }
  return current;
}

export const firstEnabled = (enabled: readonly boolean[]) => enabled.indexOf(true);
export const lastEnabled = (enabled: readonly boolean[]) => enabled.lastIndexOf(true);

/** The index a key moves to, or `null` when the key is not a movement key. */
export function rove(enabled: readonly boolean[], current: number, key: string, orientation: Orientation): number | null {
  const next = orientation === "horizontal" ? "ArrowRight" : "ArrowDown";
  const prev = orientation === "horizontal" ? "ArrowLeft" : "ArrowUp";
  if (key === next) return step(enabled, current, 1);
  if (key === prev) return step(enabled, current, -1);
  if (key === "Home") return firstEnabled(enabled);
  if (key === "End") return lastEnabled(enabled);
  return null;
}

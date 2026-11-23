// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Which tabs fit in the strip. Tabs share the width between `max` and `min`; when even `min`
// wide tabs do not fit, the rest goes behind an overflow button of `moreWidth`. The active tab
// always stays visible (it takes the last visible slot). Pure, tested in tab-fit.test.ts.

export const TAB_MIN = 132;
export const TAB_MAX = 220;
/** Width reserved for the overflow button. */
export const TAB_MORE = 84;

export interface TabFitInput {
  count: number;
  /** Index of the active tab, `-1` for none. */
  active: number;
  /** Width of the strip in px. */
  available: number;
  min?: number;
  max?: number;
  moreWidth?: number;
}

export interface TabFit {
  /** Indices of the shown tabs, in tab order. */
  visible: number[];
  /** Indices behind the overflow button, in tab order. */
  hidden: number[];
  /** Width each shown tab gets (the strip is divided evenly, within min and max). */
  width: number;
}

export function fitTabs({ count, active, available, min = TAB_MIN, max = TAB_MAX, moreWidth = TAB_MORE }: TabFitInput): TabFit {
  const all = Array.from({ length: count }, (_, i) => i);
  if (count === 0) return { visible: [], hidden: [], width: max };
  // A strip that is not measured yet shows everything (no flash of an overflow button).
  if (available <= 0 || count * min <= available) {
    return { visible: all, hidden: [], width: clamp(available / count, min, max) };
  }
  const slots = Math.max(1, Math.floor((available - moreWidth) / min));
  let visible = all.slice(0, slots);
  if (active >= slots) visible = [...all.slice(0, slots - 1), active];
  const shown = new Set(visible);
  return {
    visible,
    hidden: all.filter((i) => !shown.has(i)),
    width: clamp((available - moreWidth) / slots, min, max),
  };
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

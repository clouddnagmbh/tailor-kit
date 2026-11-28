// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Colours of the graphics (wave 8, strand E). Only tokens: a graphic follows light and dark
// with the page, and never reads a raw colour. Categories (branches, authors) cycle through the
// eight lane tokens of the commit graph; activity and age are steps of one hue mixed from the
// semantic tokens, so five steps stay apart in both themes.

/** The lane token of a category index (cycled), `var(--rt-lane-1)` … `var(--rt-lane-8)`. */
export function categoryColor(index: number): string {
  const n = ((Math.trunc(index) % 8) + 8) % 8;
  return `var(--rt-lane-${n + 1})`;
}

/** Share of the tint, per activity level 0 (none) … 4 (the busiest), in percent. */
const HEAT_STEPS = [0, 24, 46, 70, 100] as const;

/** The colour of an activity level 0 … 4: level 0 is the empty cell, 4 the full tint. */
export function heatColor(level: number): string {
  const l = Math.max(0, Math.min(4, Math.trunc(level)));
  if (l === 0) return "var(--rt-surface-sunken)";
  return `color-mix(in srgb, var(--rt-ok) ${HEAT_STEPS[l]}%, var(--rt-surface-sunken))`;
}

/** Activity levels 1 … 4 from the counts: the levels split `1 … max` evenly, so a calm repository
 *  still shows its busiest day in the full tint. Level 0 is only for a count of 0. */
export function heatLevels(counts: readonly number[]): (count: number) => number {
  const max = counts.reduce((m, c) => Math.max(m, c), 0);
  return (count) => {
    if (count <= 0 || max <= 0) return 0;
    return Math.max(1, Math.min(4, Math.ceil((count / max) * 4)));
  };
}

/** Six ages of a line, 0 (new) … 5 (old): within a week, a month, three months, half a year, a
 *  year, older. */
export const AGE_LIMITS_DAYS = [7, 30, 91, 182, 365] as const;

export function ageStep(days: number): number {
  for (let i = 0; i < AGE_LIMITS_DAYS.length; i++) if (days < AGE_LIMITS_DAYS[i]!) return i;
  return AGE_LIMITS_DAYS.length;
}

/** Six steps from the accent (new) to a calm grey (old), as tokens mixed in the page. */
export function ageColor(step: number): string {
  const s = Math.max(0, Math.min(5, Math.trunc(step)));
  const accent = [100, 78, 56, 38, 22, 10][s]!;
  return `color-mix(in srgb, var(--rt-accent) ${accent}%, var(--rt-surface-sunken))`;
}

export const AGE_LABELS = ["under a week", "under a month", "under 3 months", "under 6 months", "under a year", "over a year"] as const;

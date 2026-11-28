// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// The positions of the marks on a time axis (wave 8, strand E; V6): linear in time, but no two
// marks closer than `gap` pixels, so a burst of operations stays readable. Pure.

export interface AxisMark {
  id: string;
  /** Unix seconds. */
  time: number;
}

export interface PlacedMark {
  id: string;
  x: number;
}

/** Marks placed from the left edge `pad`; `width` is the axis' pixel width (the content is at
 *  least this wide, wider when bursts need it). Returns the placed marks (oldest first) and the
 *  content width. */
export function placeMarks(marks: readonly AxisMark[], width: number, gap = 9, pad = 16): { placed: PlacedMark[]; width: number } {
  const sorted = [...marks].sort((a, b) => a.time - b.time || (a.id < b.id ? -1 : 1));
  if (sorted.length === 0) return { placed: [], width };
  const t0 = sorted[0]!.time;
  const span = Math.max(1, sorted.at(-1)!.time - t0);
  const room = Math.max(1, width - 2 * pad);
  const placed: PlacedMark[] = [];
  let prev = -Infinity;
  for (const m of sorted) {
    const linear = pad + ((m.time - t0) / span) * room;
    const x = Math.max(linear, prev + gap);
    placed.push({ id: m.id, x });
    prev = x;
  }
  return { placed, width: Math.max(width, prev + pad) };
}

/** Ticks for the days (or hours) the axis spans: at most `max`, at round steps. */
export function dateTicks(first: number, last: number, max = 8): { time: number; label: string }[] {
  const span = Math.max(1, last - first);
  const steps = [3600, 3 * 3600, 6 * 3600, 12 * 3600, 86_400, 2 * 86_400, 7 * 86_400, 14 * 86_400, 30 * 86_400, 91 * 86_400];
  const step = steps.find((s) => span / s <= max) ?? steps.at(-1)!;
  const sub = step < 86_400;
  const out: { time: number; label: string }[] = [];
  for (let t = Math.ceil(first / step) * step; t <= last; t += step) {
    const d = new Date(t * 1000);
    out.push({
      time: t,
      label: sub ? d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false, timeZone: "UTC" }) : d.toLocaleDateString("en-US", { month: "short", day: "numeric", timeZone: "UTC" }),
    });
  }
  return out;
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// The grid of a commit calendar (wave 8, strand E; V10): which cell is which day. Days are UTC
// dates `YYYY-MM-DD`, weeks start on Monday.

export interface CalendarDay {
  day: string;
  count: number;
}

export interface CalendarCell {
  day: string;
  count: number;
  /** Column (week) and row (0 Monday … 6 Sunday). */
  week: number;
  row: number;
}

const MS = 86_400_000;

export function dayNumber(day: string): number {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  return Math.floor(Date.UTC(y, m - 1, d) / MS);
}

export function dayString(n: number): string {
  return new Date(n * MS).toISOString().slice(0, 10);
}

/** 0 Monday … 6 Sunday. */
export function weekdayOf(n: number): number {
  return (n + 3) % 7;
}

/** The cells of the last `weeks` weeks up to and including `end` (the grid ends with the week of
 *  `end`; cells after `end` are left out). */
export function calendarCells(days: readonly CalendarDay[], end: string, weeks: number): CalendarCell[] {
  const counts = new Map(days.map((d) => [d.day, d.count]));
  const endN = dayNumber(end);
  const lastMonday = endN - weekdayOf(endN);
  const firstMonday = lastMonday - (weeks - 1) * 7;
  const out: CalendarCell[] = [];
  for (let n = firstMonday; n <= endN; n++) {
    const day = dayString(n);
    out.push({ day, count: counts.get(day) ?? 0, week: Math.floor((n - firstMonday) / 7), row: weekdayOf(n) });
  }
  return out;
}

/** Labels of the months along the top: the column of each month's first visible week. */
export function monthLabels(cells: readonly CalendarCell[]): { week: number; label: string }[] {
  const fmt = new Intl.DateTimeFormat("en-US", { month: "short", timeZone: "UTC" });
  const out: { week: number; label: string }[] = [];
  let last = "";
  for (const c of cells) {
    if (c.row !== 0) continue;
    const month = c.day.slice(0, 7);
    // A month is labelled at the first Monday of the month (its first week unless the grid starts mid-month).
    if (month !== last) {
      last = month;
      const dayOfMonth = Number(c.day.slice(8));
      if (dayOfMonth <= 7 || out.length === 0) out.push({ week: c.week, label: fmt.format(new Date(`${c.day}T00:00:00Z`)) });
    }
  }
  // Two labels never touch; a partial first month gives way to the next one.
  if (out.length > 1 && out[1]!.week - out[0]!.week < 3) out.shift();
  return out.filter((l, i) => i === 0 || l.week - out[i - 1]!.week >= 3);
}

export function longDay(day: string): string {
  return new Date(`${day}T00:00:00Z`).toLocaleDateString("en-US", { weekday: "long", year: "numeric", month: "long", day: "numeric", timeZone: "UTC" });
}

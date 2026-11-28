<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The commit calendar (wave 8, strand E; V10): a year as a grid of weeks, one square a day, the
     colour the number of commits of that day. Arrow keys move over the days, Enter or a click
     selects one. The text form is a table of the months. -->
<script lang="ts">
  import { calendarCells, dayNumber, dayString, longDay, monthLabels, type CalendarDay } from "./calendar";
  import { heatColor, heatLevels } from "./color";

  let {
    days,
    end,
    weeks = 53,
    selected = null,
    onselect,
  }: {
    days: readonly CalendarDay[];
    /** The last day shown (`YYYY-MM-DD`); default: the last day with a commit, else today. */
    end?: string;
    weeks?: number;
    selected?: string | null;
    onselect?: (day: string | null) => void;
  } = $props();

  const CELL = 13;
  const GAP = 3;
  const LEFT = 30;
  const TOP = 20;

  const last = $derived(end ?? days.at(-1)?.day ?? new Date().toISOString().slice(0, 10));
  const cells = $derived(calendarCells(days, last, weeks));
  const level = $derived(heatLevels(days.map((d) => d.count)));
  const months = $derived(monthLabels(cells));
  const width = $derived(LEFT + weeks * (CELL + GAP));
  const height = TOP + 7 * (CELL + GAP) + 26;
  const total = $derived(cells.reduce((n, c) => n + c.count, 0));

  const byMonth = $derived.by(() => {
    const m = new Map<string, number>();
    for (const c of cells) m.set(c.day.slice(0, 7), (m.get(c.day.slice(0, 7)) ?? 0) + c.count);
    return [...m.entries()];
  });

  let cursor = $state<string | null>(null);
  const at = $derived(cursor ?? selected ?? last);
  const atCell = $derived(cells.find((c) => c.day === at));

  function move(e: KeyboardEvent) {
    const n = dayNumber(at);
    const first = dayNumber(cells[0]!.day);
    const lastN = dayNumber(last);
    const step = e.key === "ArrowRight" ? 7 : e.key === "ArrowLeft" ? -7 : e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step !== 0) {
      e.preventDefault();
      cursor = dayString(Math.min(lastN, Math.max(first, n + step)));
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onselect?.(selected === at ? null : at);
    } else if (e.key === "Escape" && selected) {
      onselect?.(null);
    }
  }
  const describe = (day: string, count: number) => `${count === 0 ? "No commits" : count === 1 ? "1 commit" : `${count} commits`} on ${longDay(day)}`;
</script>

<div class="calendar" data-testid="commit-calendar" data-total={total}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <svg
    {width}
    {height}
    viewBox="0 0 {width} {height}"
    role="application"
    aria-label="Commit calendar: {total} commits in {weeks} weeks. Arrow keys move over the days, Enter selects a day."
    aria-roledescription="calendar"
    tabindex="0"
    onkeydown={move}
  >
    {#each months as m (m.week)}
      <text class="month" x={LEFT + m.week * (CELL + GAP)} y={TOP - 8}>{m.label}</text>
    {/each}
    {#each ["Mon", "Wed", "Fri"] as d, i (d)}
      <text class="weekday" x="0" y={TOP + i * 2 * (CELL + GAP) + CELL - 2 + 2 * (CELL + GAP) * 0}>{d}</text>
    {/each}
    {#each cells as c (c.day)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <rect
        class="cell"
        class:selected={selected === c.day}
        class:cursor={at === c.day}
        x={LEFT + c.week * (CELL + GAP)}
        y={TOP + c.row * (CELL + GAP)}
        width={CELL}
        height={CELL}
        rx="3"
        fill={heatColor(level(c.count))}
        data-testid="calendar-day"
        data-day={c.day}
        data-count={c.count}
        onclick={() => onselect?.(selected === c.day ? null : c.day)}
      >
        <title>{describe(c.day, c.count)}</title>
      </rect>
    {/each}
    <g class="legend" transform="translate({width - 5 * (CELL + 4) - 60}, {height - 18})">
      <text class="month" x="0" y="11">Less</text>
      {#each [0, 1, 2, 3, 4] as l (l)}
        <rect x={30 + l * (CELL + 4)} y="0" width={CELL} height={CELL} rx="3" fill={heatColor(l)} />
      {/each}
      <text class="month" x={30 + 5 * (CELL + 4) + 2} y="11">More</text>
    </g>
  </svg>
  <p class="live" aria-live="polite">{atCell ? describe(atCell.day, atCell.count) : ""}</p>
  <table class="sr-only" data-testid="calendar-text">
    <caption>Commits per month</caption>
    <thead><tr><th>Month</th><th>Commits</th></tr></thead>
    <tbody>
      {#each byMonth as [month, n] (month)}
        <tr><td>{month}</td><td>{n}</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .calendar {
    max-width: 100%;
    overflow-x: auto;
    padding-bottom: var(--rt-space-2);
  }
  svg {
    display: block;
    border-radius: var(--rt-radius-md);
  }
  svg:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .month,
  .weekday {
    fill: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .cell {
    cursor: pointer;
    stroke: color-mix(in srgb, var(--rt-fg) 6%, transparent);
    stroke-width: 1;
    transition: transform var(--rt-dur-fast) var(--rt-ease);
  }
  .cell:hover {
    stroke: var(--rt-fg-muted);
  }
  svg:focus-visible .cell.cursor,
  .cell.selected {
    stroke: var(--rt-accent);
    stroke-width: 2;
  }
  .live,
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- A time axis with marks (wave 8, strand E; V6): each mark is a point in time, its height the
     number of things it changed, its shape what it is (an operation is a stem with a dot, a
     snapshot a small ring). Hovering or moving with the arrow keys previews a mark, a click or
     Enter selects it. The marks are also a list. -->
<script lang="ts">
  import { dateTicks, placeMarks } from "./timeaxis";

  export interface TimeMark {
    id: string;
    /** Unix seconds. */
    time: number;
    kind: "operation" | "snapshot";
    /** Files changed (the height); 0 is a flat mark. */
    amount: number;
    /** Shown in the tooltip and the text form. */
    label: string;
    /** The mark no longer applies (an undone operation). */
    muted?: boolean;
  }

  let {
    marks,
    selected = null,
    height = 132,
    onselect,
    onpreview,
  }: {
    marks: readonly TimeMark[];
    selected?: string | null;
    height?: number;
    onselect?: (id: string) => void;
    /** The mark under the pointer or the keyboard cursor, `null` when it leaves. */
    onpreview?: (id: string | null) => void;
  } = $props();

  const PAD_Y = 30;
  const BASE = $derived(height - 26);
  let box = $state<HTMLDivElement>();
  let outer = $state(600);

  const sorted = $derived([...marks].sort((a, b) => a.time - b.time || (a.id < b.id ? -1 : 1)));
  const layout = $derived(placeMarks(sorted, outer));
  const xOf = $derived(new Map(layout.placed.map((p) => [p.id, p.x])));
  const max = $derived(Math.max(1, ...marks.map((m) => m.amount)));
  const stem = (m: TimeMark) => (m.kind === "snapshot" ? 8 : 8 + (Math.sqrt(m.amount) / Math.sqrt(max)) * (BASE - PAD_Y - 8));
  const ticks = $derived(sorted.length > 1 ? dateTicks(sorted[0]!.time, sorted.at(-1)!.time) : []);
  const first = $derived(sorted[0]?.time ?? 0);
  const last = $derived(sorted.at(-1)?.time ?? 1);
  const tickX = (t: number) => 16 + ((t - first) / Math.max(1, last - first)) * Math.max(1, outer - 32);

  let cursor = $state<string | null>(null);
  const at = $derived(cursor ?? selected ?? sorted.at(-1)?.id ?? null);

  $effect(() => {
    if (box) {
      const ro = new ResizeObserver(() => (outer = Math.max(240, box!.clientWidth)));
      ro.observe(box);
      outer = Math.max(240, box.clientWidth);
      return () => ro.disconnect();
    }
  });
  // Keep the selected mark in view.
  $effect(() => {
    const x = selected ? xOf.get(selected) : undefined;
    if (box && x !== undefined && (x < box.scrollLeft + 20 || x > box.scrollLeft + box.clientWidth - 20)) box.scrollLeft = x - box.clientWidth / 2;
  });

  function key(e: KeyboardEvent) {
    const i = sorted.findIndex((m) => m.id === at);
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : e.key === "Home" ? -Infinity : e.key === "End" ? Infinity : 0;
    if (step !== 0) {
      e.preventDefault();
      const n = Math.max(0, Math.min(sorted.length - 1, step === Infinity ? sorted.length - 1 : step === -Infinity ? 0 : i + step));
      cursor = sorted[n]?.id ?? null;
      onpreview?.(cursor);
    } else if ((e.key === "Enter" || e.key === " ") && at) {
      e.preventDefault();
      onselect?.(at);
    }
  }
  const when = (t: number) => new Date(t * 1000).toLocaleString("en-US", { dateStyle: "medium", timeStyle: "short" });
</script>

<div class="axis" bind:this={box}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <svg
    width={layout.width}
    {height}
    viewBox="0 0 {layout.width} {height}"
    role="listbox"
    tabindex="0"
    aria-label="Timeline of {marks.length} points in time. Left and right arrows move, Enter selects."
    aria-activedescendant={at ? `mark-${at}` : undefined}
    data-testid="time-axis"
    data-marks={marks.length}
    onkeydown={key}
    onblur={() => {
      cursor = null;
      onpreview?.(null);
    }}
  >
    <line class="base" x1="0" x2={layout.width} y1={BASE} y2={BASE} />
    {#each ticks as t (t.time)}
      <line class="tick" x1={tickX(t.time)} x2={tickX(t.time)} y1={BASE} y2={BASE + 5} />
      <text class="tick-label" x={tickX(t.time)} y={BASE + 18} text-anchor="middle">{t.label}</text>
    {/each}
    {#each sorted as m (m.id)}
      {@const x = xOf.get(m.id) ?? 0}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_mouse_events_have_key_events -->
      <g
        id="mark-{m.id}"
        class="mark {m.kind}"
        class:selected={selected === m.id}
        class:cursor={cursor === m.id}
        class:muted={m.muted}
        role="option"
        tabindex="-1"
        aria-selected={selected === m.id}
        aria-label="{m.label}, {when(m.time)}"
        data-testid="time-mark"
        data-id={m.id}
        data-kind={m.kind}
        onclick={() => onselect?.(m.id)}
        onmouseenter={() => onpreview?.(m.id)}
        onmouseleave={() => onpreview?.(null)}
      >
        <title>{m.label}, {when(m.time)}{m.amount > 0 ? `, ${m.amount} ${m.amount === 1 ? "file" : "files"}` : ""}</title>
        <rect class="hit" x={x - 5} y={PAD_Y - 10} width="10" height={BASE - PAD_Y + 10} />
        {#if m.kind === "operation"}
          <line class="stem" x1={x} x2={x} y1={BASE} y2={BASE - stem(m)} />
          <circle class="dot" cx={x} cy={BASE - stem(m)} r="4.5" />
        {:else}
          <circle class="ring" cx={x} cy={BASE - 8} r="4" />
        {/if}
      </g>
    {/each}
  </svg>
</div>
<ol class="sr-only" data-testid="time-axis-text" aria-label="Points in time">
  {#each sorted as m (m.id)}<li>{when(m.time)}: {m.label}{m.amount > 0 ? `, ${m.amount} ${m.amount === 1 ? "file" : "files"} changed` : ""}</li>{/each}
</ol>

<style>
  .axis {
    overflow-x: auto;
    overflow-y: hidden;
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-sunken);
  }
  svg {
    display: block;
  }
  svg:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: -2px;
  }
  .base {
    stroke: var(--rt-border-strong);
    stroke-width: 1.5;
  }
  .tick {
    stroke: var(--rt-border-strong);
  }
  .tick-label {
    fill: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .hit {
    fill: transparent;
    cursor: pointer;
  }
  .stem {
    stroke: color-mix(in srgb, var(--rt-accent) 55%, var(--rt-surface-sunken));
    stroke-width: 2.5;
    stroke-linecap: round;
  }
  .dot {
    fill: var(--rt-accent);
    stroke: var(--rt-surface-sunken);
    stroke-width: 2;
  }
  .ring {
    fill: var(--rt-surface-1);
    stroke: var(--rt-fg-muted);
    stroke-width: 2;
  }
  .mark:hover .stem,
  .mark.cursor .stem,
  .mark.selected .stem {
    stroke: var(--rt-accent);
    stroke-width: 3.5;
  }
  .mark:hover .ring,
  .mark.cursor .ring,
  .mark.selected .ring {
    stroke: var(--rt-accent);
    fill: var(--rt-accent-subtle);
  }
  .mark.selected .dot {
    stroke: var(--rt-fg);
    stroke-width: 2.5;
  }
  .mark.muted {
    opacity: 0.45;
  }
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

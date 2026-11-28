<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The branch map (wave 8, strand E; V1): the branches as the lines of a transit map. A branch is
     a line, a fork or a merge a curve between two lines, a station a commit where lines meet. The
     arrows at the end of a line say what is still to push (up) and pull (down); old branches fade.
     Click selects a branch, double click (or Shift+Enter) opens it. Plain SVG with the labels as
     real buttons over it; the same facts are in a list for screen readers. -->
<script lang="ts">
  import { categoryColor } from "./color";
  import { describeLine, layoutBranchMap, type BranchMapInput, type MapLine } from "./branchmap";

  let {
    input,
    selected = null,
    compact = false,
    onselect,
    onopen,
  }: {
    input: BranchMapInput;
    /** The selected branch (its name). */
    selected?: string | null;
    /** The small form for a sidebar: no labels, no dates, tighter. */
    compact?: boolean;
    onselect?: (name: string) => void;
    /** Double click or Shift+Enter: open the branch (switch to it). */
    onopen?: (name: string) => void;
  } = $props();

  const COL = $derived(compact ? 14 : 44);
  const ROW = $derived(compact ? 12 : 42);
  const LEFT = $derived(compact ? 8 : 28);
  const TOP = $derived(compact ? 8 : 44);
  const RIGHT = $derived(compact ? 8 : 24);
  const labelSlots = $derived((name: string) => (compact ? 1 : Math.ceil((name.length * 6.8 + 76) / COL)));

  const layout = $derived(layoutBranchMap({ ...input, labelSlots }));
  const rows = $derived(layout.maxRow - layout.minRow + 1);
  const farRight = $derived(Math.max(0, ...layout.lines.map((l) => l.to + labelSlots(l.name))));
  const width = $derived(LEFT + Math.max(layout.columns, farRight) * COL + RIGHT);
  const height = $derived(TOP + rows * ROW + (compact ? 4 : 16));
  const x = (slot: number) => LEFT + slot * COL;
  const y = (row: number) => TOP + (row - layout.minRow) * ROW + ROW / 2;

  let hovered = $state<string | null>(null);
  let scroller = $state<HTMLDivElement>();

  // Open at the newest commits: the right edge.
  $effect(() => {
    width;
    if (scroller && !compact) scroller.scrollLeft = scroller.scrollWidth;
  });

  const lineByName = $derived(new Map(layout.lines.map((l) => [l.name, l])));
  const focus = $derived(hovered ?? selected);
  const related = $derived.by(() => {
    if (!focus) return null;
    const set = new Set<string>([focus]);
    for (const k of layout.links) {
      if (k.fromLine === focus) set.add(k.toLine);
      if (k.toLine === focus) set.add(k.fromLine);
    }
    return set;
  });
  const dimmed = (l: MapLine) => (related ? !related.has(l.name) : l.dim);

  /** The line from its first to its last station, starting where its fork curve ends. */
  function span(l: MapLine): { x1: number; x2: number } {
    const forked = l.fork !== null && l.fork.oid !== l.tip;
    return { x1: x(l.from) + (forked ? COL * 0.62 : 0), x2: x(l.to) };
  }

  function curve(x1: number, y1: number, x2: number, y2: number): string {
    const dx = Math.max(COL * 0.5, Math.abs(x2 - x1) * 0.5);
    return `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
  }
  function fork(l: { from: { x: number; row: number }; to: { x: number; row: number } }): string {
    const x1 = x(l.from.x);
    const x2 = x1 + COL * 0.62;
    return `M ${x1} ${y(l.from.row)} C ${x1 + COL * 0.34} ${y(l.from.row)}, ${x2 - COL * 0.34} ${y(l.to.row)}, ${x2} ${y(l.to.row)}`;
  }

  // The date of a few stations along the top.
  const ticks = $derived.by(() => {
    if (compact) return [] as { x: number; label: string }[];
    const out: { x: number; label: string }[] = [];
    let last = -1;
    const fmt = new Intl.DateTimeFormat("en-US", { month: "short", year: "numeric" });
    for (const s of layout.stations) {
      if (s.time <= 0) continue;
      const label = fmt.format(new Date(s.time * 1000));
      if (s.x - last >= 4 && (out.length === 0 || out[out.length - 1]!.label !== label)) {
        out.push({ x: s.x, label });
        last = s.x;
      }
    }
    return out;
  });

  function key(e: KeyboardEvent, name: string) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) onopen?.(name);
      else onselect?.(name);
    }
  }
</script>

{#if layout.lines.length === 0}
  <p class="none" data-testid="branchmap-empty">There are no branches to draw yet.</p>
{:else}
  <div class="map" class:compact bind:this={scroller} data-testid="branch-map" data-lines={layout.lines.length} data-rows={rows}>
    <div class="canvas" style:width="{width}px" style:height="{height}px">
      <svg {width} {height} viewBox="0 0 {width} {height}" role="img" aria-label="Branch map: {layout.lines.length} branches" focusable="false">
        {#each ticks as t (t.x)}
          <line class="tick" x1={x(t.x)} x2={x(t.x)} y1={TOP - 12} y2={height - 6} />
          <text class="tick-label" x={x(t.x) + 6} y={TOP - 18}>{t.label}</text>
        {/each}
        {#each layout.links as k, i (i)}
          {@const from = lineByName.get(k.fromLine)}
          {@const to = lineByName.get(k.toLine)}
          {#if from && to}
            <path
              class="link"
              class:dim={dimmed(from) && dimmed(to)}
              class:stack={k.kind === "fork" && to.stacked}
              d={k.kind === "fork" ? fork(k) : curve(x(k.from.x), y(k.from.row), x(k.to.x), y(k.to.row))}
              style:stroke={categoryColor(k.kind === "fork" ? to.color : from.color)}
              data-kind={k.kind}
            />
          {/if}
        {/each}
        {#each layout.lines as l (l.name)}
          {@const s = span(l)}
          <g class="line" class:dim={dimmed(l)} class:current={l.current} class:trunk={l.name === layout.trunk} style:--line={categoryColor(l.color)}>
            {#if l.current}<circle class="halo" cx={s.x2} cy={y(l.row)} r={compact ? 7 : 13} />{/if}
            <line class="rail" x1={s.x1} x2={Math.max(s.x1, s.x2)} y1={y(l.row)} y2={y(l.row)} />
            {#if !compact}
              {#each l.stations as st (st)}
                {#if st !== l.to}<circle class="station" cx={x(st)} cy={y(l.row)} r="4" />{/if}
              {/each}
            {/if}
            <circle class="tip" cx={s.x2} cy={y(l.row)} r={compact ? 3 : 6} />
          </g>
        {/each}
      </svg>

      {#if !compact}
        {#each layout.lines as l (l.name)}
          <button
            type="button"
            class="label"
            class:selected={selected === l.name}
            class:dim={dimmed(l)}
            class:current={l.current}
            class:stacked={l.stacked}
            style:left="{x(l.to) + 14}px"
            style:top="{y(l.row) - 12}px"
            style:--line={categoryColor(l.color)}
            data-testid="branchmap-line"
            data-branch={l.name}
            data-row={l.row}
            aria-pressed={selected === l.name}
            aria-label="{l.name}. {describeLine(l, layout.trunk)}"
            title="{l.name}: {describeLine(l, layout.trunk)}"
            onclick={() => onselect?.(l.name)}
            ondblclick={() => onopen?.(l.name)}
            onkeydown={(e) => key(e, l.name)}
            onmouseenter={() => (hovered = l.name)}
            onmouseleave={() => (hovered = null)}
            onfocus={() => (hovered = l.name)}
            onblur={() => (hovered = null)}
          >
            <span class="dot" aria-hidden="true"></span>
            <span class="name">{l.name}</span>
            {#if l.ahead > 0}<span class="pill up" data-testid="branchmap-ahead">↑{l.ahead}</span>{/if}
            {#if l.behind > 0}<span class="pill down" data-testid="branchmap-behind">↓{l.behind}</span>{/if}
          </button>
        {/each}
      {/if}
    </div>
  </div>
  <!-- The same facts for screen readers (and for tests). -->
  <ul class="sr-only" data-testid="branchmap-text" aria-label="Branches">
    {#each layout.lines as l (l.name)}
      <li>{describeLine(l, layout.trunk)}</li>
    {/each}
  </ul>
{/if}

<style>
  .none {
    margin: 0;
    padding: var(--rt-space-6);
    color: var(--rt-fg-muted);
  }
  .map {
    position: relative;
    overflow: auto;
    min-height: 0;
    background:
      radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--rt-border) 70%, transparent) 1px, transparent 0) 0 0 / 22px 22px,
      var(--rt-surface-1);
    border-radius: var(--rt-radius-lg);
  }
  .map.compact {
    background: none;
    border-radius: var(--rt-radius-md);
  }
  .canvas {
    position: relative;
  }
  svg {
    position: absolute;
    inset: 0;
    overflow: visible;
  }
  .tick {
    stroke: var(--rt-border);
    stroke-dasharray: 2 5;
  }
  .tick-label {
    fill: var(--rt-fg-subtle);
    font: var(--rt-font-caption);
    letter-spacing: var(--rt-tracking-caps);
  }
  .rail {
    stroke: var(--line);
    stroke-width: 6;
    stroke-linecap: round;
    transition: opacity var(--rt-dur-fast) var(--rt-ease);
  }
  .trunk .rail {
    stroke-width: 8;
  }
  .compact .rail {
    stroke-width: 3;
  }
  .compact .trunk .rail {
    stroke-width: 4;
  }
  .line.dim,
  .link.dim {
    opacity: 0.32;
  }
  .station {
    fill: var(--rt-surface-1);
    stroke: var(--line);
    stroke-width: 2.5;
  }
  .tip {
    fill: var(--line);
    stroke: var(--rt-surface-1);
    stroke-width: 2.5;
  }
  .halo {
    fill: color-mix(in srgb, var(--line) 22%, transparent);
    stroke: var(--line);
    stroke-width: 1.5;
  }
  .link {
    fill: none;
    stroke-width: 4;
    stroke-linecap: round;
    opacity: 0.9;
  }
  .link[data-kind="merge"] {
    stroke-dasharray: 1 7;
    stroke-width: 4.5;
  }
  .link.stack {
    stroke-dasharray: 8 5;
  }
  .label {
    position: absolute;
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-2);
    height: 24px;
    max-width: 280px;
    padding: 0 var(--rt-space-4) 0 var(--rt-space-3);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-2);
    color: var(--rt-fg);
    font: var(--rt-font-small);
    box-shadow: var(--rt-elev-1);
    cursor: pointer;
    transition:
      opacity var(--rt-dur-fast) var(--rt-ease),
      border-color var(--rt-dur-fast) var(--rt-ease);
  }
  .label:hover,
  .label.selected {
    border-color: var(--line);
  }
  .label.selected {
    background: color-mix(in srgb, var(--line) 12%, var(--rt-surface-2));
    font-weight: var(--rt-weight-semibold);
  }
  .label.current .name {
    font-weight: var(--rt-weight-semibold);
  }
  .label.dim {
    opacity: 0.5;
  }
  .label.stacked {
    border-style: dashed;
  }
  .label:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--line);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill {
    flex: none;
    padding: 0 5px;
    border-radius: var(--rt-radius-pill);
    font: var(--rt-font-caption);
    font-variant-numeric: tabular-nums;
  }
  .pill.up {
    background: var(--rt-ok-bg);
    color: var(--rt-ok-fg);
  }
  .pill.down {
    background: var(--rt-warn-bg);
    color: var(--rt-warn-fg);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
</style>

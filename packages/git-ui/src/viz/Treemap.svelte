<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- A treemap (wave 8, strand E; V13): the largest things as areas, the colour the kind (a file
     extension, a folder). Click or Enter selects a cell. The text form is the same list as a
     table, in the order the caller gave. -->
<script lang="ts">
  import { categoryColor } from "./color";
  import { squarify } from "./treemap";

  export interface TreemapCell {
    id: string;
    label: string;
    size: number;
    /** Cells of one group share a colour. */
    group?: string;
    /** Shown after the label ("12 MB"). */
    value?: string;
    /** More for the tooltip and the text form. */
    detail?: string;
  }

  let {
    items,
    selected = null,
    unit = "",
    onselect,
  }: {
    items: readonly TreemapCell[];
    selected?: string | null;
    unit?: string;
    onselect?: (id: string) => void;
  } = $props();

  const W = 100;
  const H = 56;
  const rects = $derived(squarify(items.map((i) => ({ id: i.id, size: i.size })), W, H));
  const byId = $derived(new Map(items.map((i) => [i.id, i])));
  const groups = $derived([...new Set(items.map((i) => i.group ?? ""))].sort());
  const colour = (c: TreemapCell) => categoryColor(groups.indexOf(c.group ?? ""));
</script>

<div class="treemap" style:aspect-ratio="{W} / {H}" data-testid="treemap" data-cells={rects.length}>
  {#each rects as r (r.id)}
    {@const c = byId.get(r.id)!}
    {@const big = r.w * r.h > 90}
    <button
      type="button"
      class="cell"
      class:selected={selected === c.id}
      class:big
      style:left="{(r.x / W) * 100}%"
      style:top="{(r.y / H) * 100}%"
      style:width="{(r.w / W) * 100}%"
      style:height="{(r.h / H) * 100}%"
      style:--c={colour(c)}
      data-testid="treemap-cell"
      data-id={c.id}
      title="{c.label}{c.value ? `, ${c.value}` : ""}{c.detail ? `, ${c.detail}` : ""}"
      aria-pressed={selected === c.id}
      aria-label="{c.label}{c.value ? `, ${c.value}` : ""}"
      onclick={() => onselect?.(c.id)}
    >
      {#if big}
        <span class="label">{c.label}</span>
        {#if c.value}<span class="value">{c.value}</span>{/if}
      {/if}
    </button>
  {/each}
</div>
{#if groups.length > 1 || groups[0]}
  <ul class="legend" aria-hidden="true">
    {#each groups as g, i (g)}
      <li><span class="swatch" style:background={categoryColor(i)}></span>{g || "other"}</li>
    {/each}
  </ul>
{/if}
<table class="sr-only" data-testid="treemap-text">
  <caption>{unit ? `Largest items by ${unit}` : "Largest items"}</caption>
  <thead><tr><th>Name</th><th>Size</th><th>More</th></tr></thead>
  <tbody>
    {#each items as c (c.id)}
      <tr><td>{c.label}</td><td>{c.value ?? c.size}</td><td>{c.detail ?? ""}</td></tr>
    {/each}
  </tbody>
</table>

<style>
  .treemap {
    position: relative;
    width: 100%;
    max-height: 70vh;
    border-radius: var(--rt-radius-lg);
    overflow: hidden;
    background: var(--rt-surface-sunken);
  }
  .cell {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: flex-start;
    gap: 1px;
    padding: var(--rt-space-2) var(--rt-space-3);
    border: 2px solid var(--rt-surface-1);
    border-radius: 6px;
    background: color-mix(in srgb, var(--c) 30%, var(--rt-surface-1));
    color: var(--rt-fg);
    font: var(--rt-font-small);
    text-align: left;
    overflow: hidden;
    cursor: pointer;
    transition: background var(--rt-dur-fast) var(--rt-ease);
  }
  .cell:hover {
    background: color-mix(in srgb, var(--c) 48%, var(--rt-surface-1));
  }
  .cell.selected {
    outline: 2px solid var(--rt-accent);
    outline-offset: -3px;
    z-index: 1;
  }
  .cell:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: -3px;
    z-index: 1;
  }
  .label {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: var(--rt-weight-medium);
  }
  .value {
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    font-variant-numeric: tabular-nums;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-4);
    margin: var(--rt-space-4) 0 0;
    padding: 0;
    list-style: none;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-2);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 3px;
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

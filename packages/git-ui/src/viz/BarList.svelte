<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- Rows with a number and a bar (wave 8, strand E; V10: commits per author). The rows keep the
     order they come in: the caller sorts by name, never by size, so the list is a record and not a
     league table. -->
<script lang="ts">
  import { categoryColor } from "./color";

  export interface BarItem {
    id: string;
    label: string;
    value: number;
    /** Under the label ("ada@example.org"). */
    detail?: string;
  }

  let {
    items,
    unit = "commits",
    selected = null,
    onselect,
  }: {
    items: readonly BarItem[];
    unit?: string;
    selected?: string | null;
    onselect?: (id: string) => void;
  } = $props();

  const max = $derived(Math.max(1, ...items.map((i) => i.value)));
  const total = $derived(items.reduce((n, i) => n + i.value, 0));
  const initials = (s: string) =>
    s
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]!.toUpperCase())
      .join("") || "?";
</script>

<ul class="bars" data-testid="bar-list" data-total={total}>
  {#each items as it, i (it.id)}
    <li>
      <button type="button" class="row" class:selected={selected === it.id} aria-pressed={selected === it.id} data-testid="bar-row" data-id={it.id} data-value={it.value} onclick={() => onselect?.(it.id)} aria-label="{it.label}: {it.value} {unit}">
        <span class="avatar" style:background={categoryColor(i)} aria-hidden="true">{initials(it.label)}</span>
        <span class="who">
          <span class="name">{it.label}</span>
          {#if it.detail}<span class="detail">{it.detail}</span>{/if}
        </span>
        <span class="track" aria-hidden="true"><span class="bar" style:width="{(it.value / max) * 100}%" style:background={categoryColor(i)}></span></span>
        <span class="value">{it.value.toLocaleString("en-US")}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .bars {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .row {
    display: grid;
    grid-template-columns: 28px minmax(120px, 220px) minmax(60px, 1fr) 56px;
    align-items: center;
    gap: var(--rt-space-4);
    width: 100%;
    padding: var(--rt-space-2) var(--rt-space-3);
    border: 0;
    border-radius: var(--rt-radius-md);
    background: none;
    color: var(--rt-fg);
    font: var(--rt-font-body);
    text-align: left;
    cursor: pointer;
  }
  .row:hover {
    background: var(--rt-bg-hover);
  }
  .row.selected {
    background: var(--rt-bg-selected);
  }
  .row:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: -2px;
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    color: var(--rt-fg-on-solid);
    font: var(--rt-font-caption);
    font-weight: var(--rt-weight-bold);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name,
  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail {
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .track {
    height: 8px;
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-sunken);
    overflow: hidden;
  }
  .bar {
    display: block;
    height: 100%;
    border-radius: var(--rt-radius-pill);
    opacity: 0.85;
  }
  .value {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-weight: var(--rt-weight-medium);
  }
</style>

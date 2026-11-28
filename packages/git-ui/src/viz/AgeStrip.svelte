<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- How old each branch is (wave 8, strand E; V12): a row per branch with a bar on a time scale,
     the colour one of six age steps. The rows keep the order they come in. -->
<script lang="ts">
  import { AGE_LABELS, ageColor, ageStep } from "./color";

  export interface AgeItem {
    id: string;
    label: string;
    /** Days since its last commit. */
    days: number;
    /** What it is: "merged into main", "upstream gone". */
    state?: string;
  }

  let {
    items,
    maxDays = 365,
  }: {
    items: readonly AgeItem[];
    /** The right edge of the scale. */
    maxDays?: number;
  } = $props();

  // Fourth-root scale: a day and a month both show, a year fills the bar.
  const share = (d: number) => Math.max(0.03, Math.min(1, Math.pow(Math.max(0, d) / maxDays, 0.4)));
  const word = (d: number) => (d === 0 ? "today" : d === 1 ? "1 day ago" : d < 60 ? `${d} days ago` : d < 730 ? `${Math.round(d / 30)} months ago` : `${Math.round(d / 365)} years ago`);
</script>

<div class="strip" data-testid="age-strip" role="list">
  {#each items as it (it.id)}
    {@const step = ageStep(it.days)}
    <div class="row" role="listitem" data-testid="age-row" data-id={it.id} data-step={step} aria-label="{it.label}, last commit {word(it.days)}{it.state ? `, ${it.state}` : ''}">
      <span class="name" title={it.label}>{it.label}</span>
      <span class="track" aria-hidden="true"><span class="bar" style:width="{share(it.days) * 100}%" style:background={ageColor(step)}></span></span>
      <span class="when">{word(it.days)}</span>
      {#if it.state}<span class="state">{it.state}</span>{/if}
    </div>
  {/each}
  <div class="legend" aria-hidden="true">
    {#each AGE_LABELS as l, i (l)}<span><i style:background={ageColor(i)}></i>{l}</span>{/each}
  </div>
</div>

<style>
  .strip {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-2);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(120px, 220px) minmax(80px, 1fr) 96px auto;
    align-items: center;
    gap: var(--rt-space-4);
    min-height: 26px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
  }
  .track {
    height: 10px;
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-sunken);
    overflow: hidden;
  }
  .bar {
    display: block;
    height: 100%;
    border-radius: var(--rt-radius-pill);
  }
  .when {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .state {
    padding: 1px 8px;
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-sunken);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    white-space: nowrap;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-5);
    margin-top: var(--rt-space-3);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-2);
  }
  .legend i {
    width: 12px;
    height: 8px;
    border-radius: 3px;
  }
</style>

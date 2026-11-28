<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- Columns for a fixed set of buckets, in their natural order (wave 8, strand E; V10: commits per
     weekday and per hour). Every bar has its number on hover; the buckets are never reordered by
     size. -->
<script lang="ts">
  let {
    values,
    labels,
    title,
    height = 96,
    showEvery = 1,
  }: {
    values: readonly number[];
    labels: readonly string[];
    /** What the columns count ("Commits per weekday"). */
    title: string;
    height?: number;
    /** Label every n-th column. */
    showEvery?: number;
  } = $props();

  const max = $derived(Math.max(1, ...values));
  const total = $derived(values.reduce((a, b) => a + b, 0));
</script>

<figure class="chart" data-testid="column-chart" data-title={title} data-total={total}>
  <div class="bars" style:height="{height}px" style:--n={values.length} role="img" aria-label="{title}: {labels.map((l, i) => `${l} ${values[i] ?? 0}`).join(', ')}">
    {#each values as v, i (i)}
      <div class="col" title="{labels[i]}: {v}" data-testid="column" data-value={v}>
        <span class="value" aria-hidden="true">{v > 0 ? v : ""}</span>
        <span class="bar" class:empty={v === 0} style:height="{(v / max) * 100}%"></span>
      </div>
    {/each}
  </div>
  <div class="axis" style:--n={values.length} aria-hidden="true">
    {#each labels as l, i (i)}<span>{i % showEvery === 0 ? l : ""}</span>{/each}
  </div>
  <figcaption>{title}</figcaption>
</figure>

<style>
  .chart {
    margin: 0;
    min-width: 0;
  }
  .bars,
  .axis {
    display: grid;
    grid-template-columns: repeat(var(--n), minmax(0, 1fr));
    gap: 3px;
  }
  .bars {
    align-items: end;
    border-bottom: 1px solid var(--rt-border-strong);
  }
  .col {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    height: 100%;
  }
  .bar {
    display: block;
    min-height: 2px;
    border-radius: 3px 3px 0 0;
    background: color-mix(in srgb, var(--rt-accent) 78%, var(--rt-surface-sunken));
    transition: height var(--rt-dur-normal) var(--rt-ease);
  }
  .bar.empty {
    background: var(--rt-surface-sunken);
  }
  .col:hover .bar {
    background: var(--rt-accent);
  }
  .value {
    position: absolute;
    bottom: calc(100% - 0px);
    left: 0;
    right: 0;
    text-align: center;
    font: var(--rt-font-caption);
    color: var(--rt-fg-muted);
    font-variant-numeric: tabular-nums;
    opacity: 0;
    pointer-events: none;
  }
  .col:hover .value {
    opacity: 1;
  }
  .axis {
    margin-top: var(--rt-space-2);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    text-align: center;
  }
  figcaption {
    margin-top: var(--rt-space-2);
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
</style>

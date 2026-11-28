<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- A sparkline (wave 8, strand E; V15): a few numbers as a small area chart with the last value
     marked. The numbers are also the accessible name. -->
<script lang="ts">
  let {
    values,
    width = 84,
    height = 22,
    label = "Activity",
    unit = "",
  }: {
    values: readonly number[];
    width?: number;
    height?: number;
    /** What the numbers are ("Commits per week, last 4 weeks"). */
    label?: string;
    unit?: string;
  } = $props();

  const PAD = 3;
  const max = $derived(Math.max(1, ...values));
  const pts = $derived(
    values.map((v, i) => ({
      x: values.length <= 1 ? width / 2 : PAD + (i * (width - 2 * PAD)) / (values.length - 1),
      y: height - PAD - (v / max) * (height - 2 * PAD),
    })),
  );
  const line = $derived(pts.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(" "));
  const area = $derived(pts.length > 1 ? `${line} L ${pts.at(-1)!.x.toFixed(1)} ${height - PAD} L ${pts[0]!.x.toFixed(1)} ${height - PAD} Z` : "");
  const last = $derived(pts.at(-1));
  const text = $derived(`${label}: ${values.join(", ")}${unit ? ` ${unit}` : ""}`);
</script>

<svg class="spark" {width} {height} viewBox="0 0 {width} {height}" role="img" aria-label={text} data-testid="sparkline" data-values={values.join(",")}>
  <title>{text}</title>
  {#if values.every((v) => v === 0)}
    <line x1={PAD} x2={width - PAD} y1={height - PAD} y2={height - PAD} class="flat" />
  {:else}
    {#if area}<path d={area} class="area" />{/if}
    <path d={line} class="line" />
    {#if last}<circle cx={last.x} cy={last.y} r="2.4" class="end" />{/if}
  {/if}
</svg>

<style>
  .spark {
    display: block;
    flex: none;
    overflow: visible;
  }
  .line {
    fill: none;
    stroke: var(--rt-accent);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .area {
    fill: color-mix(in srgb, var(--rt-accent) 16%, transparent);
  }
  .end {
    fill: var(--rt-accent);
    stroke: var(--rt-surface-1);
    stroke-width: 1.2;
  }
  .flat {
    stroke: var(--rt-border-strong);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-dasharray: 1 4;
  }
</style>

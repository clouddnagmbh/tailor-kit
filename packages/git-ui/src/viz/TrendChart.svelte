<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- A number over time as an area chart (wave 8, strand E; V10: commits over the weeks). Moving the
     pointer or the arrow keys over it reads one point. The points are also a table. -->
<script lang="ts">
  export interface TrendPoint {
    /** Start of the bucket, Unix seconds. */
    time: number;
    value: number;
  }

  let {
    points,
    title,
    height = 140,
    bucket = "week",
  }: {
    points: readonly TrendPoint[];
    title: string;
    height?: number;
    /** What one point covers ("week"), for the tooltip. */
    bucket?: string;
  } = $props();

  const PAD = { l: 34, r: 10, t: 12, b: 24 };
  let width = $state(640);
  let box = $state<HTMLDivElement>();
  $effect(() => {
    if (!box) return;
    const ro = new ResizeObserver(() => (width = Math.max(240, box!.clientWidth)));
    ro.observe(box);
    width = Math.max(240, box.clientWidth);
    return () => ro.disconnect();
  });

  const max = $derived(Math.max(1, ...points.map((p) => p.value)));
  const niceMax = $derived(Math.ceil(max / 4) * 4 || 4);
  const t0 = $derived(points[0]?.time ?? 0);
  const t1 = $derived(points.at(-1)?.time ?? 1);
  const xs = (t: number) => PAD.l + (points.length <= 1 ? (width - PAD.l - PAD.r) / 2 : ((t - t0) / Math.max(1, t1 - t0)) * (width - PAD.l - PAD.r));
  const ys = (v: number) => PAD.t + (1 - v / niceMax) * (height - PAD.t - PAD.b);
  const line = $derived(points.map((p, i) => `${i === 0 ? "M" : "L"} ${xs(p.time).toFixed(1)} ${ys(p.value).toFixed(1)}`).join(" "));
  const area = $derived(points.length > 1 ? `${line} L ${xs(t1)} ${height - PAD.b} L ${xs(t0)} ${height - PAD.b} Z` : "");
  const grid = $derived([0, 0.5, 1].map((f) => ({ v: Math.round(niceMax * f), y: ys(niceMax * f) })));
  const dateOf = (t: number) => new Date(t * 1000).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric", timeZone: "UTC" });

  let hot = $state<number | null>(null);
  function move(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const x = e.clientX - r.left;
    let best = 0;
    let d = Infinity;
    points.forEach((p, i) => {
      const dx = Math.abs(xs(p.time) - x);
      if (dx < d) {
        d = dx;
        best = i;
      }
    });
    hot = best;
  }
  function key(e: KeyboardEvent) {
    if (points.length === 0) return;
    if (e.key === "ArrowRight") hot = Math.min(points.length - 1, (hot ?? -1) + 1);
    else if (e.key === "ArrowLeft") hot = Math.max(0, (hot ?? points.length) - 1);
    else return;
    e.preventDefault();
  }
  const hotPoint = $derived(hot === null ? null : (points[hot] ?? null));
</script>

<div class="trend" bind:this={box} data-testid="trend-chart" data-points={points.length}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <svg {width} {height} viewBox="0 0 {width} {height}" role="img" tabindex="0" aria-label="{title}: {points.length} {bucket}s, from {points[0] ? dateOf(points[0].time) : 'none'}" onpointermove={move} onpointerleave={() => (hot = null)} onkeydown={key} onblur={() => (hot = null)}>
    {#each grid as g (g.v)}
      <line class="grid" x1={PAD.l} x2={width - PAD.r} y1={g.y} y2={g.y} />
      <text class="axis" x={PAD.l - 6} y={g.y + 4} text-anchor="end">{g.v}</text>
    {/each}
    {#if points.length > 0}
      <text class="axis" x={PAD.l} y={height - 6}>{dateOf(t0)}</text>
      <text class="axis" x={width - PAD.r} y={height - 6} text-anchor="end">{dateOf(t1)}</text>
    {/if}
    {#if area}<path class="area" d={area} />{/if}
    {#if points.length > 0}<path class="line" d={line} />{/if}
    {#if hotPoint}
      <line class="cursor" x1={xs(hotPoint.time)} x2={xs(hotPoint.time)} y1={PAD.t} y2={height - PAD.b} />
      <circle class="point" cx={xs(hotPoint.time)} cy={ys(hotPoint.value)} r="4.5" />
    {/if}
  </svg>
  <p class="read" aria-live="polite" data-testid="trend-read">{hotPoint ? `${hotPoint.value} ${hotPoint.value === 1 ? "commit" : "commits"} in the ${bucket} of ${dateOf(hotPoint.time)}` : ""}</p>
  <table class="sr-only" data-testid="trend-text">
    <caption>{title}</caption>
    <thead><tr><th>{bucket} of</th><th>Commits</th></tr></thead>
    <tbody>{#each points as p (p.time)}<tr><td>{dateOf(p.time)}</td><td>{p.value}</td></tr>{/each}</tbody>
  </table>
</div>

<style>
  .trend {
    position: relative;
    min-width: 0;
  }
  svg {
    display: block;
    width: 100%;
    touch-action: none;
  }
  svg:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
    border-radius: var(--rt-radius-md);
  }
  .grid {
    stroke: var(--rt-border);
    stroke-dasharray: 2 4;
  }
  .axis {
    fill: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .area {
    fill: color-mix(in srgb, var(--rt-accent) 16%, transparent);
  }
  .line {
    fill: none;
    stroke: var(--rt-accent);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .cursor {
    stroke: var(--rt-fg-subtle);
    stroke-dasharray: 3 3;
  }
  .point {
    fill: var(--rt-accent);
    stroke: var(--rt-surface-1);
    stroke-width: 2;
  }
  .read {
    min-height: 18px;
    margin: var(--rt-space-2) 0 0;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
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

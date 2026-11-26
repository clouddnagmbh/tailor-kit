<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A placeholder while content loads: a block (`width`, `height`), a circle, or `lines` of
     text (the last one shorter). Use it when the shape of the result is known; else Spinner. -->
<script lang="ts">
  let {
    width = "100%",
    height = "12px",
    circle = false,
    lines = 0,
    label = "Loading",
  }: { width?: string; height?: string; circle?: boolean; lines?: number; label?: string } = $props();
</script>

{#if lines > 0}
  <span class="rt-skeleton-lines" role="status" aria-label={label}>
    {#each { length: lines } as _, i (i)}
      <span class="rt-skeleton" style:width={i === lines - 1 && lines > 1 ? "62%" : width} style:height="10px"></span>
    {/each}
  </span>
{:else}
  <span
    class="rt-skeleton"
    class:circle
    role="status"
    aria-label={label}
    style:width={circle ? height : width}
    style:height
  ></span>
{/if}

<style>
  .rt-skeleton-lines {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
    width: 100%;
  }
  .rt-skeleton {
    display: block;
    border-radius: var(--rt-radius-sm);
    background: linear-gradient(
        90deg,
        transparent 0%,
        var(--rt-skeleton-shine) 50%,
        transparent 100%
      )
      0 0 / 200% 100% no-repeat,
      var(--rt-skeleton);
    animation: shimmer 1.4s var(--rt-ease) infinite;
  }
  .circle {
    border-radius: 50%;
  }
  @keyframes shimmer {
    from {
      background-position: 150% 0, 0 0;
    }
    to {
      background-position: -50% 0, 0 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .rt-skeleton {
      animation: none;
    }
  }
</style>

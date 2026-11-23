<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Two versions of an image (wave 15, strand B): side by side, faded into each other with a
     slider, or as the difference (what changed shows up bright, the rest is black). A new or
     deleted image shows the one side. Sizes in pixels and bytes come with it. -->
<script lang="ts">
  import type { GitBlob } from "../../commands/types";
  import { SegmentedControl } from "../../ui";
  import { dataUrl, formatSize, sizeChange } from "./model";

  let { before, after }: { before: GitBlob | null; after: GitBlob | null } = $props();

  let mode = $state("side");
  let fade = $state(50);
  let beforeSize = $state<[number, number] | null>(null);
  let afterSize = $state<[number, number] | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let diffError = $state<string | null>(null);

  const both = $derived(before !== null && after !== null);
  const options = $derived([
    { value: "side", label: "Side by side", testid: "image-mode-side" },
    { value: "fade", label: "Fade", disabled: !both, testid: "image-mode-fade" },
    { value: "difference", label: "Difference", disabled: !both, testid: "image-mode-difference" },
  ]);

  function size(e: Event, set: (v: [number, number]) => void) {
    const img = e.currentTarget as HTMLImageElement;
    set([img.naturalWidth, img.naturalHeight]);
  }

  function load(src: string): Promise<HTMLImageElement> {
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => reject(new Error("The image cannot be read."));
      img.src = src;
    });
  }

  // The difference: draw the old image, then the new one with the "difference" blend mode.
  $effect(() => {
    const target = canvas;
    if (mode !== "difference" || !target || !before || !after) return;
    const [a, b] = [dataUrl(before), dataUrl(after)];
    let live = true;
    diffError = null;
    Promise.all([load(a), load(b)]).then(
      ([old, next]) => {
        if (!live) return;
        const ctx = target.getContext("2d");
        if (!ctx) {
          diffError = "This browser cannot draw the difference.";
          return;
        }
        target.width = Math.max(old.naturalWidth, next.naturalWidth);
        target.height = Math.max(old.naturalHeight, next.naturalHeight);
        ctx.fillStyle = "#000";
        ctx.fillRect(0, 0, target.width, target.height);
        ctx.drawImage(old, 0, 0);
        ctx.globalCompositeOperation = "difference";
        ctx.drawImage(next, 0, 0);
      },
      (e: unknown) => {
        if (live) diffError = e instanceof Error ? e.message : String(e);
      },
    );
    return () => {
      live = false;
    };
  });

  const dims = (s: [number, number] | null) => (s ? `${s[0]} × ${s[1]} px` : "");
</script>

<div class="image-diff" data-testid="image-diff" data-mode={mode}>
  <div class="bar">
    {#if both}
      <SegmentedControl size="sm" label="Image comparison" {options} bind:value={mode} />
    {/if}
    {#if mode === "fade" && both}
      <label class="fade">
        <span>Old</span>
        <input type="range" min="0" max="100" bind:value={fade} aria-label="Fade between old and new" data-testid="image-fade" />
        <span>New</span>
      </label>
    {/if}
    {#if before && after}
      <span class="delta" data-testid="image-delta">{formatSize(before.size)} → {formatSize(after.size)} ({sizeChange(before.size, after.size)})</span>
    {/if}
  </div>

  {#if mode === "side" || !both}
    <div class="side">
      {#if before}
        <figure data-testid="image-before">
          <figcaption>{after ? "Before" : "Deleted"} · {formatSize(before.size)} {dims(beforeSize)}</figcaption>
          <div class="frame"><img src={dataUrl(before)} alt="Before" onload={(e) => size(e, (v) => (beforeSize = v))} /></div>
        </figure>
      {/if}
      {#if after}
        <figure data-testid="image-after">
          <figcaption>{before ? "After" : "New"} · {formatSize(after.size)} {dims(afterSize)}</figcaption>
          <div class="frame"><img src={dataUrl(after)} alt="After" onload={(e) => size(e, (v) => (afterSize = v))} /></div>
        </figure>
      {/if}
    </div>
  {:else if mode === "fade" && before && after}
    <div class="stack frame" data-testid="image-fade-stack">
      <img src={dataUrl(before)} alt="Before" />
      <img class="top" src={dataUrl(after)} alt="After" style:opacity={fade / 100} />
    </div>
  {:else}
    <div class="frame">
      {#if diffError}
        <p class="error" role="alert">{diffError}</p>
      {:else}
        <canvas bind:this={canvas} data-testid="image-difference" aria-label="Difference between the two images"></canvas>
      {/if}
    </div>
  {/if}
</div>

<style>
  .image-diff {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
    padding: var(--rt-space-5);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-5);
  }
  .fade {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
  }
  .delta {
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
  }
  .side {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: var(--rt-space-5);
  }
  figure {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
    min-width: 0;
  }
  figcaption {
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
  }
  .frame {
    display: flex;
    justify-content: center;
    padding: var(--rt-space-5);
    overflow: auto;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-md);
    /* A checkerboard shows transparent parts. */
    background:
      repeating-conic-gradient(var(--rt-surface-sunken) 0% 25%, var(--rt-surface-2) 0% 50%) 50% / 16px 16px;
  }
  img,
  canvas {
    max-width: 100%;
    height: auto;
    image-rendering: auto;
  }
  .stack {
    position: relative;
    display: grid;
    place-items: center;
  }
  .stack img {
    grid-area: 1 / 1;
  }
  .error {
    margin: 0;
    color: var(--rt-danger-fg);
  }
</style>

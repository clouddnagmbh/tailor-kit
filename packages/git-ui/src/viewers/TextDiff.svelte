<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The line diff of two texts with the words that differ marked inside paired lines (wave 15,
     strand B; `model.ts`). Diffs of up to VIRTUAL_FROM rows are drawn whole. Longer ones (a 10 MB
     file) are virtualised: a viewport of fixed-height rows draws only the rows in view, so the
     frame time does not depend on the size of the file (wave 3, strand G). -->
<script lang="ts">
  import DiffMinimap from "../diff/DiffMinimap.svelte";
  import Words from "../diff/Words.svelte";
  import { EmptyState } from "@tailor-kit/ui";
  import { getGitHost } from "../host";
  import { diffRows } from "./model";

  let scrollIds = 0;

  let {
    before,
    after,
    onhunks,
  }: {
    before: string | null;
    after: string | null;
    /** How many hunks (blocks of changed lines) the diff has, whenever it is computed. */
    onhunks?: (count: number) => void;
  } = $props();

  /** Up to this many rows the diff is drawn whole (text wraps, rows may differ in height). */
  const VIRTUAL_FROM = 2000;
  /** Rows drawn beyond the viewport on each side, so a fast scroll never shows a gap. */
  const OVERSCAN = 20;
  const SIGN = { context: " ", added: "+", removed: "-" } as const;

  const rows = $derived(diffRows(before, after));
  $effect(() => onhunks?.(rows.filter((r) => r.kind === "hunk").length));

  const virtual = $derived(rows.length > VIRTUAL_FROM);
  // The host may switch on a strip with the changed blocks beside the diff (wave 8). Such a diff
  // scrolls inside a frame of its own, so the strip has something to show and to move.
  let host: ReturnType<typeof getGitHost> | null = null;
  try {
    host = getGitHost();
  } catch {
    // Outside a Git host (a gallery, a test): no strip.
  }
  const minimapOn = $derived(host?.viz?.diffMinimap === true);
  const framed = $derived(minimapOn && !virtual && rows.length > 40);
  const scrollId = `diff-scroll-${++scrollIds}`;
  const kinds = $derived(minimapOn ? rows.map((r) => (r.kind === "added" ? "a" : r.kind === "removed" ? "r" : r.kind === "hunk" ? "h" : "c")) : []);
  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let scrollHeight = $state(0);
  let viewHeight = $state(480);
  /** The height of one row in px: the theme's small leading. */
  let rowHeight = $state(16);

  $effect(() => {
    if (!viewport) return;
    const px = parseFloat(getComputedStyle(viewport).getPropertyValue("--rt-leading-small"));
    if (px > 0) rowHeight = px;
  });

  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN));
  const last = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewHeight) / rowHeight) + OVERSCAN));
  const visible = $derived(virtual ? rows.slice(first, last) : rows);
  $effect(() => {
    rows.length;
    if (viewport) scrollHeight = viewport.scrollHeight;
  });
  function seek(fraction: number) {
    if (viewport) viewport.scrollTop = fraction * viewport.scrollHeight;
  }
</script>

{#snippet line(row: (typeof rows)[number])}
  {#if row.kind === "hunk"}
    <div class="hunk" data-testid="text-hunk">{row.text}</div>
  {:else}
    <div class="line {row.kind}" data-kind={row.kind} data-testid="text-line">
      <span class="no" aria-hidden="true">{row.old ?? ""}</span>
      <span class="no" aria-hidden="true">{row.new ?? ""}</span>
      <span class="sign" aria-hidden="true">{SIGN[row.kind]}</span>
      <span class="text">{#if row.words}<Words segments={row.words} side={row.kind === "removed" ? "old" : "new"} />{:else}{row.text}{/if}</span>
    </div>
  {/if}
{/snippet}

<div class="text-diff" data-testid="text-diff">
  {#if rows.length === 0}
    <div data-testid="text-diff-empty"><EmptyState icon="file-text" title="No line changes" text="The text is the same on both sides." size="sm" /></div>
  {:else if virtual || framed}
    <div class="frame" class:with-map={minimapOn}>
      <!-- A scrollable region must be reachable by keyboard to be scrolled. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        class="viewport"
        id={scrollId}
        data-testid={virtual ? "text-diff-virtual" : "text-diff-framed"}
        data-rows={rows.length}
        data-drawn={visible.length}
        role="region"
        aria-label="Diff, {rows.length} rows"
        tabindex="0"
        bind:this={viewport}
        bind:clientHeight={viewHeight}
        onscroll={(e) => {
          scrollTop = e.currentTarget.scrollTop;
          scrollHeight = e.currentTarget.scrollHeight;
        }}
        style:--row-height="{rowHeight}px"
      >
        {#if virtual}
          <div class="sizer" style:height="{rows.length * rowHeight}px">
            <div class="window" style:transform="translateY({first * rowHeight}px)">
              {#each visible as row}
                {@render line(row)}
              {/each}
            </div>
          </div>
        {:else}
          {#each rows as row}
            {@render line(row)}
          {/each}
        {/if}
      </div>
      {#if minimapOn}
        <DiffMinimap {kinds} {scrollTop} scrollHeight={virtual ? rows.length * rowHeight : scrollHeight} clientHeight={viewHeight} controls={scrollId} onseek={seek} />
      {/if}
    </div>
  {:else}
    {#each rows as row}
      {@render line(row)}
    {/each}
  {/if}
</div>

<style>
  .text-diff {
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
    line-height: var(--rt-leading-small);
  }
  .hunk {
    padding: var(--rt-space-2) var(--rt-space-5);
    color: var(--rt-fg-muted);
    background: var(--rt-surface-sunken);
  }
  .line {
    display: grid;
    grid-template-columns: 5ch 5ch 2ch minmax(0, 1fr);
    padding: 0 var(--rt-space-5) 0 var(--rt-space-3);
  }
  .line.added {
    background: var(--rt-diff-add-bg);
  }
  .line.removed {
    background: var(--rt-diff-del-bg);
  }
  .no {
    padding-right: var(--rt-space-2);
    text-align: right;
    color: var(--rt-fg-muted);
    user-select: none;
  }
  .sign {
    color: var(--rt-fg-muted);
    user-select: none;
  }
  .added .sign {
    color: var(--rt-ok-fg);
  }
  .removed .sign {
    color: var(--rt-danger-fg);
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .viewport {
    height: min(70vh, 48rem);
    overflow: auto;
    contain: strict;
  }
  .frame.with-map {
    display: flex;
    height: min(70vh, 48rem);
  }
  .frame.with-map .viewport {
    flex: 1;
    min-width: 0;
    height: 100%;
  }
  .sizer {
    position: relative;
    min-width: max-content;
  }
  .window {
    position: absolute;
    inset: 0 0 auto 0;
    will-change: transform;
  }
  /* Virtualised rows have one height: no wrapping. */
  .viewport .hunk,
  .viewport .line {
    height: var(--row-height);
    box-sizing: border-box;
    overflow: hidden;
    white-space: pre;
  }
  .viewport .text {
    white-space: pre;
    overflow-wrap: normal;
  }
</style>

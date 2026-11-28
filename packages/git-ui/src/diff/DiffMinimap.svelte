<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The minimap of a text diff (wave 8, strand E; V8): a strip beside the diff that shows the whole
     file at once, the added and removed lines as coloured marks and the part in view as a frame. A
     click or drag jumps there. It is a scrollbar for the diff (arrow keys, Page Up and Down). -->
<script lang="ts">
  /** One letter per row: `a` added, `r` removed, `h` hunk header, anything else context. */
  let {
    kinds,
    scrollTop,
    scrollHeight,
    clientHeight,
    controls,
    onseek,
  }: {
    kinds: readonly string[];
    scrollTop: number;
    scrollHeight: number;
    clientHeight: number;
    /** The id of the scroll area. */
    controls?: string;
    /** Scroll so that this fraction (0 … 1) of the content is at the top. */
    onseek: (fraction: number) => void;
  } = $props();

  /** At most this many marks, however long the diff is. */
  const BUCKETS = 360;
  const n = $derived(kinds.length);
  const marks = $derived.by(() => {
    const size = Math.max(1, Math.ceil(n / BUCKETS));
    const out: { y: number; h: number; kind: "a" | "r" | "ar" }[] = [];
    for (let start = 0; start < n; start += size) {
      let a = 0;
      let r = 0;
      for (let i = start; i < Math.min(n, start + size); i++) {
        if (kinds[i] === "a") a++;
        else if (kinds[i] === "r") r++;
      }
      if (a + r > 0) out.push({ y: start / n, h: Math.min(size, n - start) / n, kind: a > 0 && r > 0 ? "ar" : a > 0 ? "a" : "r" });
    }
    return out;
  });
  const blocks = $derived.by(() => {
    let count = 0;
    let inside = false;
    for (const k of kinds) {
      const changed = k === "a" || k === "r";
      if (changed && !inside) count++;
      inside = changed;
    }
    return count;
  });
  const frameTop = $derived(scrollHeight > 0 ? scrollTop / scrollHeight : 0);
  const frameHeight = $derived(scrollHeight > 0 ? Math.min(1, clientHeight / scrollHeight) : 1);
  const percent = $derived(Math.round((scrollHeight > clientHeight ? scrollTop / (scrollHeight - clientHeight) : 0) * 100));

  let strip = $state<HTMLDivElement>();
  let dragging = false;
  function seek(e: PointerEvent) {
    if (!strip) return;
    const r = strip.getBoundingClientRect();
    const f = (e.clientY - r.top) / r.height;
    // The frame centres on the point.
    onseek(Math.max(0, Math.min(1 - frameHeight, f - frameHeight / 2)));
  }
  function key(e: KeyboardEvent) {
    const page = frameHeight;
    const to = e.key === "ArrowDown" ? frameTop + page / 8 : e.key === "ArrowUp" ? frameTop - page / 8 : e.key === "PageDown" ? frameTop + page : e.key === "PageUp" ? frameTop - page : e.key === "Home" ? 0 : e.key === "End" ? 1 : null;
    if (to === null) return;
    e.preventDefault();
    onseek(Math.max(0, Math.min(1 - frameHeight, to)));
  }
</script>

<div
  class="minimap"
  bind:this={strip}
  role="scrollbar"
  tabindex="0"
  aria-orientation="vertical"
  aria-label="Overview of the changes: {blocks} changed {blocks === 1 ? 'block' : 'blocks'}"
  aria-controls={controls}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={percent}
  data-testid="diff-minimap"
  data-blocks={blocks}
  onpointerdown={(e) => {
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    seek(e);
  }}
  onpointermove={(e) => dragging && seek(e)}
  onpointerup={() => (dragging = false)}
  onkeydown={key}
>
  {#each marks as m, i (i)}
    <span class="mark {m.kind}" style:top="{m.y * 100}%" style:height="max(2px, {m.h * 100}%)" data-testid="minimap-mark"></span>
  {/each}
  <span class="frame" style:top="{frameTop * 100}%" style:height="{frameHeight * 100}%" data-testid="minimap-frame"></span>
</div>

<style>
  .minimap {
    position: relative;
    flex: none;
    width: 14px;
    height: 100%;
    border-left: 1px solid var(--rt-border);
    background: var(--rt-surface-sunken);
    cursor: pointer;
    touch-action: none;
  }
  .minimap:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: -2px;
  }
  .mark {
    position: absolute;
    left: 2px;
    right: 2px;
    border-radius: 1px;
  }
  .mark.a {
    background: var(--rt-ok);
  }
  .mark.r {
    background: var(--rt-danger);
  }
  .mark.ar {
    background: linear-gradient(90deg, var(--rt-danger) 50%, var(--rt-ok) 50%);
  }
  .frame {
    position: absolute;
    left: 0;
    right: 0;
    min-height: 8px;
    border: 1px solid var(--rt-fg-muted);
    border-radius: 2px;
    background: color-mix(in srgb, var(--rt-fg) 8%, transparent);
    pointer-events: none;
  }
</style>

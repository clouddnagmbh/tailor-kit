<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Text with the words that differ marked (wave 14, strand H, `worddiff.ts`). -->
<script lang="ts">
  import type { Segment } from "./worddiff";

  let { segments, side }: { segments: Segment[]; side: "old" | "new" } = $props();
</script>

<span class="words {side}">{#each segments as s, i (i)}{#if s.same}{s.text}{:else}<mark>{s.text}</mark>{/if}{/each}</span>

<style>
  .words {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  mark {
    padding: 0 var(--rt-space-1);
    margin: 0 calc(-1 * var(--rt-space-1));
    border-radius: var(--rt-radius-sm);
    color: inherit;
  }
  .old mark {
    background: var(--rt-diff-del-word);
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--rt-danger-fg) 55%, transparent);
  }
  .new mark {
    background: var(--rt-diff-add-word);
  }
</style>

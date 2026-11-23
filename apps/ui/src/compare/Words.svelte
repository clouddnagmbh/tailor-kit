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
    color: inherit;
    border-radius: 2px;
  }
  .old mark {
    background: var(--rt-diff-del-word);
  }
  .new mark {
    background: var(--rt-diff-add-word);
  }
</style>

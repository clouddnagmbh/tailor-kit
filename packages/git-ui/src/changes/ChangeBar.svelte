<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The +/- bar of a changed file (wave 8, strand C, V7): small cells in proportion, green for added
     and red for removed lines; `label` gives the screen reader the numbers. Colours from the tokens,
     so it follows light and dark. -->
<script lang="ts">
  import { barCells, barText, type LineCounts } from "./bar";

  let { added, removed, cells = 5, label = true }: { added: number; removed: number; cells?: number; label?: boolean } = $props();

  const counts = $derived<LineCounts>({ added, removed });
  const bar = $derived(barCells(counts, cells));
  const kinds = $derived([...Array<string>(bar.add).fill("add"), ...Array<string>(bar.del).fill("del"), ...Array<string>(bar.rest).fill("rest")]);
</script>

<span class="bar" role={label ? "img" : undefined} aria-label={label ? barText(counts) : undefined} aria-hidden={label ? undefined : "true"} data-testid="change-bar" data-added={added} data-removed={removed}>
  {#each kinds as kind, i (i)}<span class="cell {kind}"></span>{/each}
</span>

<style>
  .bar {
    display: inline-flex;
    gap: 1px;
    align-items: center;
    flex: none;
  }
  .cell {
    width: 6px;
    height: 10px;
    border-radius: 1px;
    background: var(--rt-border);
  }
  .cell.add {
    background: var(--rt-ok-fg);
  }
  .cell.del {
    background: var(--rt-danger-fg);
  }
</style>

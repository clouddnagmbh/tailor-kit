<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The line diff of two texts with the words that differ marked inside paired lines (wave 15,
     strand B; `model.ts`). Long diffs show the first rows and offer the rest. -->
<script lang="ts">
  import Words from "../../compare/Words.svelte";
  import { Button, EmptyState } from "../../ui";
  import { diffRows } from "./model";

  let { before, after }: { before: string | null; after: string | null } = $props();

  const ROW_LIMIT = 2000;
  const SIGN = { context: " ", added: "+", removed: "-" } as const;

  const rows = $derived(diffRows(before, after));
  let all = $state(false);
  const shown = $derived(all ? rows : rows.slice(0, ROW_LIMIT));
</script>

<div class="text-diff" data-testid="text-diff">
  {#if rows.length === 0}
    <div data-testid="text-diff-empty"><EmptyState icon="file-text" title="No line changes" text="The text is the same on both sides." size="sm" /></div>
  {:else}
    {#each shown as row, i (i)}
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
    {/each}
    {#if !all && rows.length > ROW_LIMIT}
      <div class="more">
        <Button size="sm" data-testid="text-diff-more" onclick={() => (all = true)}>Show {rows.length - ROW_LIMIT} more rows</Button>
      </div>
    {/if}
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
  .more {
    padding: var(--rt-space-4) var(--rt-space-5);
  }
</style>

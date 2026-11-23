<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The Git panel's tools as a compact grid of tiles: an icon, a short label, the full
     explanation as tooltip and accessible description. -->
<script lang="ts">
  import type { ToolTile } from "./model";

  let { tiles }: { tiles: ToolTile[] } = $props();
  const GLYPH: Record<string, string> = {
    graph: "⑂",
    compare: "⇄",
    history: "◷",
    bisect: "⌖",
    review: "✓",
    contract: "§",
    shield: "⛨",
    tool: "•",
  };
</script>

<section class="tools" aria-label="Tools">
  <header class="section-head"><h3>Tools</h3></header>
  <ul data-testid="git-tools">
    {#each tiles as t (t.id)}
      <li>
        <button class="tile" data-testid="git-tool-{t.id}" title={t.tip} aria-label="{t.label}: {t.tip}" onclick={() => t.run()}>
          <span class="icon" aria-hidden="true">{GLYPH[t.icon] ?? GLYPH.tool}</span>
          <span class="label">{t.label}</span>
        </button>
      </li>
    {/each}
  </ul>
</section>

<style>
  ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(68px, 1fr));
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    min-width: 0;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    width: 100%;
    height: 46px;
    padding: 4px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font: inherit;
    cursor: pointer;
  }
  .tile:hover {
    background: var(--rt-bg-hover);
  }
  .icon {
    font-size: 16px;
    line-height: 1;
    color: var(--rt-fg-muted);
  }
  .label {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--rt-size-small);
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The Git panel's tools as a compact grid: icon and short label, the full explanation as
     tooltip and accessible name. -->
<script lang="ts">
  import { Button, Section, Tooltip } from "../../ui";
  import type { ToolTile } from "./model";

  let { tiles }: { tiles: ToolTile[] } = $props();
</script>

<Section title="Tools">
  <ul class="tools" data-testid="git-tools">
    {#each tiles as t (t.id)}
      <li>
        <Tooltip text={t.tip}>
          <Button block size="sm" icon={t.icon} data-testid="git-tool-{t.id}" aria-label="{t.label}: {t.tip}" onclick={() => t.run()}>{t.label}</Button>
        </Tooltip>
      </li>
    {/each}
  </ul>
</Section>

<style>
  .tools {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
    gap: var(--rt-space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li,
  li :global(.rt-tooltip-anchor) {
    display: flex;
    min-width: 0;
  }
  li :global(.rt-tooltip-anchor) {
    flex: 1;
  }
  li :global(.rt-btn) {
    justify-content: flex-start;
  }
</style>

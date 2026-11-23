<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 14, strand F: the commit graph of the active collection's repository (all branches,
     remote branches, tags, HEAD) as coloured lanes (`--rt-lane-*`), virtualised, with the commit
     details (semantic diff for collection files, line diff for the rest) and context menus that
     open the wizards (`../wizards/open.ts`). Placeholder until strand F. -->
<script lang="ts">
  let { select }: { select?: string } = $props();
  const selected = $derived(select ?? null);
</script>

<p class="empty" data-testid="git-graph" data-select={selected}>The history view arrives with wave 14.</p>

<style>
  .empty {
    color: var(--rt-fg-muted);
    padding: calc(var(--rt-gap) * 2);
  }
</style>

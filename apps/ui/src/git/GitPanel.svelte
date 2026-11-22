<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Sidebar view "Git": status, diff, commit, branches, push/pull, conflicts, clone.
     Owner: wave 4, strand A2. `onopen` shows a collection the backend has opened (clone). -->
<script lang="ts">
  import type { CollectionInfo } from "../commands/backend";

  let _props: { onopen: (info: CollectionInfo) => Promise<void> } = $props();

</script>

<div class="placeholder" data-testid="git-panel">Git arrives in wave 4.</div>

<style>
  .placeholder {
    padding: 12px;
    color: var(--rt-fg-muted);
  }
</style>

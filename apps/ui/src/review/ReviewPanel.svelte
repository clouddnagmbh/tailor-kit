<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Pull and merge requests: list, open, semantic diff (diff/SemanticDiff.svelte), run the
     changed requests, comment, approve, CI status (wave 12, strand E). Interface placeholder
     with the props the other strands rely on; the strand replaces the body. -->
<script lang="ts">
  let props: { number?: number } = $props();
</script>

<p class="not-yet" data-testid="wave12-placeholder" data-props={JSON.stringify(props)}>Pull-request review arrives in wave 12.</p>

<style>
  .not-yet {
    padding: calc(var(--rt-gap) * 2);
    color: var(--rt-fg-muted);
  }
</style>

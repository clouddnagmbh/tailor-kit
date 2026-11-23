<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15 (interface): the 3-way view of a conflicted file that is not a collection file, resolved with GitApi.resolveText. Placeholder until strand D builds it. -->
<script lang="ts">
  import { Button, EmptyState } from "../../ui";

  let { path, onclose }: { path: string; onclose: () => void } = $props();
</script>

<EmptyState icon="merge" title="Resolve conflict" text="This view arrives with wave 15 (strand D)." data-testid="git-view-pending" data-target={JSON.stringify({ path })}>
  {#snippet actions()}
    <Button onclick={onclose}>Back</Button>
  {/snippet}
</EmptyState>

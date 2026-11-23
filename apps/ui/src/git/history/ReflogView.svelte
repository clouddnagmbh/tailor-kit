<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15 (interface): where a branch or HEAD pointed before, to bring back lost work. Placeholder until strand C builds it. -->
<script lang="ts">
  import { EmptyState } from "../../ui";

  let { name }: { name?: string } = $props();
</script>

<EmptyState icon="undo" title="Lost work" text="This view arrives with wave 15 (strand C)." data-testid="git-view-pending" data-target={JSON.stringify({ name })} />

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15 (interface): every commit that changed a file, following renames. Placeholder until strand C builds it. -->
<script lang="ts">
  import { EmptyState } from "../../ui";

  let { path }: { path: string } = $props();
</script>

<EmptyState icon="history" title="File history" text="This view arrives with wave 15 (strand C)." data-testid="git-view-pending" data-target={JSON.stringify({ path })} />

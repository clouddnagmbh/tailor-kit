<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15 (interface): who last changed each line of a file. Placeholder until strand C builds it. -->
<script lang="ts">
  import { EmptyState } from "../../ui";

  let { path, rev }: { path: string; rev?: string } = $props();
</script>

<EmptyState icon="history" title="Blame" text="This view arrives with wave 15 (strand C)." data-testid="git-view-pending" data-target={JSON.stringify({ path, rev })} />

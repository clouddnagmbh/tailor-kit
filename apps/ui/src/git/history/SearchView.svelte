<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15 (interface): searching commit messages, authors, paths and content. Placeholder until strand C builds it. -->
<script lang="ts">
  import { EmptyState } from "../../ui";

  import type { GitHistoryQuery } from "../../commands/types";

  let { query }: { query?: Partial<GitHistoryQuery> } = $props();
</script>

<EmptyState icon="search" title="Search history" text="This view arrives with wave 15 (strand C)." data-testid="git-view-pending" data-target={JSON.stringify(query ?? {})} />

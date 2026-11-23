<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15 (interface): one file of the repository at a revision or in the working tree (text, images, binary metadata). Placeholder until strand B builds it. -->
<script lang="ts">
  import { EmptyState } from "../../ui";

  let { path, rev }: { path: string; rev?: string } = $props();
</script>

<EmptyState icon="file-text" title="File" text="This view arrives with wave 15 (strand B)." data-testid="git-view-pending" data-target={JSON.stringify({ path, rev })} />

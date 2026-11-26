<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Text with the words that differ marked. The component moved to `packages/git-ui` (step K3);
     this keeps the old path. -->
<script lang="ts">
  import Words from "@tailor-kit/git-ui/diff/Words.svelte";
  import type { Segment } from "./worddiff";

  let { segments, side }: { segments: Segment[]; side: "old" | "new" } = $props();
</script>

<Words {segments} {side} />

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Command palette (Mod+K) of the shell; the component is shared (@tailor-kit/shell), the ids
     keep the `rt` prefix. -->
<script lang="ts">
  import { CommandPalette } from "@tailor-kit/shell";

  import type { Command } from "./commands";

  let { open = $bindable(false), commands }: { open?: boolean; commands: Command[] } = $props();
</script>

<CommandPalette bind:open {commands} idPrefix="rt" />

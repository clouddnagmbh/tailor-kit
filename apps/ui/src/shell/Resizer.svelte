<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The drag handle between two panes lives in @tailor-kit/shell; this keeps the old import. -->
<script lang="ts">
  import { Resizer } from "@tailor-kit/shell";
  import type { ComponentProps } from "svelte";

  let props: ComponentProps<typeof Resizer> = $props();
</script>

<Resizer {...props} />

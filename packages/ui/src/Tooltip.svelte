<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A tooltip around its content. On a single element, prefer the action:
     `<button use:tooltip={"Copy URL"}>`. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { Placement } from "./position";
  import { tooltip } from "./tooltip";

  let {
    text,
    shortcut,
    placement = "top",
    delay,
    children,
  }: { text: string; shortcut?: string; placement?: Placement; delay?: number; children: Snippet } = $props();
</script>

<span class="rt-tooltip-anchor" use:tooltip={{ text, shortcut, placement, delay }}>{@render children()}</span>

<style>
  .rt-tooltip-anchor {
    display: inline-flex;
    max-width: 100%;
  }
</style>

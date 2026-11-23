<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A keyboard shortcut, `keys="Mod+Enter"`: ⌘ ↵ on macOS, Ctrl ↵ elsewhere. -->
<script lang="ts">
  import { isMacPlatform } from "../shell/shortcuts";
  import { formatKeys } from "./keys";

  let { keys, size = "md" }: { keys: string; size?: "sm" | "md" } = $props();
  const mac = isMacPlatform();
  const parts = $derived(formatKeys(keys, mac));
</script>

<span class="rt-kbd {size}" aria-label={keys.replaceAll("Mod", mac ? "Command" : "Control")}>
  {#each parts as part, i (i)}<kbd>{part}</kbd>{/each}
</span>

<style>
  .rt-kbd {
    display: inline-flex;
    gap: var(--rt-space-1);
    vertical-align: middle;
  }
  kbd {
    display: inline-grid;
    place-items: center;
    min-width: 18px;
    height: 18px;
    padding: 0 var(--rt-space-2);
    box-sizing: border-box;
    border: 1px solid var(--rt-border-strong);
    border-bottom-width: 2px;
    border-radius: var(--rt-radius-sm);
    background: var(--rt-surface-2);
    color: var(--rt-fg-muted);
    font: var(--rt-weight-medium) var(--rt-text-caption) / 1 var(--rt-font-ui);
  }
  .sm kbd {
    min-width: 16px;
    height: 16px;
    border-bottom-width: 1px;
    font-size: 10px;
  }
</style>

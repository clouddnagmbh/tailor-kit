<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The footer of a Dialog: actions on the right (primary last), optional `start` content on the
     left (a "Learn more" link, a checkbox). Put it at the end of the dialog's content; it sticks
     to the bottom edge while the body scrolls. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { start, children }: { start?: Snippet; children: Snippet } = $props();
</script>

<footer class="rt-dialog-footer">
  {#if start}<div class="start">{@render start()}</div>{/if}
  <div class="end">{@render children()}</div>
</footer>

<style>
  .rt-dialog-footer {
    position: sticky;
    bottom: calc(-1 * var(--dlg-py, var(--rt-space-6)));
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    margin: var(--rt-space-6) calc(-1 * var(--dlg-px, var(--rt-space-7))) calc(-1 * var(--dlg-py, var(--rt-space-6)));
    padding: var(--rt-space-5) var(--dlg-px, var(--rt-space-7));
    border-top: 1px solid var(--rt-border);
    border-radius: 0 0 var(--rt-radius-lg) var(--rt-radius-lg);
    background: var(--rt-bg-panel);
  }
  .start {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    min-width: 0;
  }
  .end {
    display: flex;
    gap: var(--rt-space-4);
    margin-left: auto;
  }
</style>

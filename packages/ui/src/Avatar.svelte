<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A person as initials on a colour derived from the name (stable on every device). No
     images: Free never loads anything from third-party servers. -->
<script lang="ts">
  import { colourIndex, initials } from "./avatar";

  let { name, size = "md", title }: { name: string; size?: "sm" | "md" | "lg"; title?: string } = $props();
</script>

<span class="rt-avatar {size} c{colourIndex(name)}" role="img" aria-label={name} title={title ?? name}>{initials(name)}</span>

<style>
  .rt-avatar {
    --c: var(--rt-lane-1);
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--c) 18%, var(--rt-surface-2));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 30%, transparent);
    color: color-mix(in srgb, var(--c) 82%, var(--rt-fg));
    font: var(--rt-weight-semibold) 10px / 1 var(--rt-font-ui);
    letter-spacing: 0.02em;
    user-select: none;
  }
  .sm {
    width: 18px;
    height: 18px;
    font-size: 8px;
  }
  .lg {
    width: 32px;
    height: 32px;
    font-size: var(--rt-text-small);
  }
  .c2 {
    --c: var(--rt-lane-2);
  }
  .c3 {
    --c: var(--rt-lane-3);
  }
  .c4 {
    --c: var(--rt-lane-4);
  }
  .c5 {
    --c: var(--rt-lane-5);
  }
  .c6 {
    --c: var(--rt-lane-6);
  }
  .c7 {
    --c: var(--rt-lane-7);
  }
  .c8 {
    --c: var(--rt-lane-8);
  }
</style>

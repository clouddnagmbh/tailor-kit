<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A semantic icon (ui/icons.ts). Size and stroke come from the tokens; decorative unless a
     `label` is given. Unknown names (from extensions) get the fallback icon. -->
<script lang="ts">
  import { iconFor, type IconName } from "./icons";

  let {
    name,
    size = "md",
    label,
    fallback = "spark",
    class: className = "",
  }: {
    name: IconName | (string & {});
    size?: "sm" | "md" | "lg";
    /** Accessible name; leave out for icons next to a text label. */
    label?: string;
    fallback?: IconName;
    class?: string;
  } = $props();

  const Glyph = $derived(iconFor(name, fallback));
</script>

<Glyph
  class="rt-icon rt-icon-{size} {className}"
  aria-hidden={label ? undefined : "true"}
  aria-label={label}
  role={label ? "img" : undefined}
  focusable="false"
/>

<style>
  :global(.rt-icon) {
    flex: none;
    stroke-width: var(--rt-icon-stroke);
    vertical-align: middle;
  }
  :global(.rt-icon-sm) {
    width: var(--rt-icon-sm);
    height: var(--rt-icon-sm);
  }
  :global(.rt-icon-md) {
    width: var(--rt-icon-md);
    height: var(--rt-icon-md);
  }
  :global(.rt-icon-lg) {
    width: var(--rt-icon-lg);
    height: var(--rt-icon-lg);
  }
</style>

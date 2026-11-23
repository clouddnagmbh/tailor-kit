<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The one hint of the extensions for a place (wave 14, strand C): above the request editor and
     at the top of an expanded collection node. At most one, the first not dismissed; a close
     button dismisses it per device (`ExtensionHint.scope`). Interface stub: shows nothing. -->
<script lang="ts">
  import type { HintTarget } from "../extensions";

  let { target }: { target: HintTarget | null } = $props();
  // Strand C: the hint is the first of `extensionHintsFor(getExtensions(), target)` that is not
  // dismissed.
  const place = $derived(target?.place ?? null);
</script>

{#if place}{/if}

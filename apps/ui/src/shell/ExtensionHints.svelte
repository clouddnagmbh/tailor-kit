<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The one hint of the extensions for a place (wave 14, strand C): above the request editor and
     at the top of an expanded collection node. At most one, the first not dismissed; a close
     button dismisses it (per device or per target, `ExtensionHint.scope`). A calm line: never
     modal, never blocking. -->
<script lang="ts">
  import type { HintTarget } from "../extensions";
  import { Banner, Button } from "../ui";
  import { dismissalKey, extensionHintsFor, pickHint } from "./extension-hints";
  import { getExtensions } from "./extension-tabs";
  import { hintStore } from "./hint-store";

  let { target }: { target: HintTarget | null } = $props();

  const provided = getExtensions();
  const dismissed = hintStore.dismissed;
  const shown = $derived(target ? pickHint(extensionHintsFor(provided, target), target, $dismissed) : null);

  function dismiss() {
    if (shown && target) hintStore.dismiss(dismissalKey(shown, target));
  }

  function act(action: { run: () => unknown; dismiss?: boolean }) {
    try {
      void Promise.resolve(action.run()).catch((e) => console.warn("RequestTailor: hint action failed:", e));
    } catch (e) {
      console.warn("RequestTailor: hint action failed:", e);
    }
    if (action.dismiss) dismiss();
  }
</script>

{#if shown && target}
  {#key shown.key}
    <div class="hint" data-testid="extension-hint" data-hint={shown.key}>
      <Banner tone="info" dismissible dismissLabel="Dismiss hint" ondismiss={dismiss}>
        {shown.hint.text}
        {#snippet actions()}
          {#each (shown.hint.actions ?? []).slice(0, 2) as action, i (i)}
            <Button size="sm" data-testid="hint-action" onclick={() => act(action)}>{action.label}</Button>
          {/each}
        {/snippet}
      </Banner>
    </div>
  {/key}
{/if}

<style>
  .hint {
    padding: 0 0 var(--rt-space-4);
  }
  /* A calm line above the URL bar: tighter than a full banner. */
  .hint :global(.rt-banner) {
    align-items: center;
    padding: var(--rt-space-3) var(--rt-space-4);
    border-radius: var(--rt-radius-md);
    font: var(--rt-font-small);
  }
  .hint :global(.rt-banner .dismiss) {
    margin: 0;
  }
  .hint :global(.rt-banner .lead) {
    margin-top: 0;
  }
</style>

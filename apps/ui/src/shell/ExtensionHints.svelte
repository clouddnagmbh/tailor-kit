<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The one hint of the extensions for a place (wave 14, strand C): above the request editor and
     at the top of an expanded collection node. At most one, the first not dismissed; a close
     button dismisses it (per device or per target, `ExtensionHint.scope`). A calm line: never
     modal, never blocking. -->
<script lang="ts">
  import type { HintTarget } from "../extensions";
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
    <aside class="hint" class:compact={target.place === "collection"} aria-label="Hint" data-testid="extension-hint" data-hint={shown.key}>
      <span class="mark" aria-hidden="true"></span>
      <p>{shown.hint.text}</p>
      <div class="actions">
        {#each (shown.hint.actions ?? []).slice(0, 2) as action, i (i)}
          <button type="button" class="act" data-testid="hint-action" onclick={() => act(action)}>{action.label}</button>
        {/each}
        <button type="button" class="close" data-testid="hint-dismiss" aria-label="Dismiss hint" title="Dismiss" onclick={dismiss}>
          <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true"><path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none" /></svg>
        </button>
      </div>
    </aside>
  {/key}
{/if}

<style>
  .hint {
    display: flex;
    align-items: center;
    gap: var(--rt-gap);
    min-height: var(--rt-bar-height);
    padding: 4px var(--rt-side-inset);
    background: var(--rt-bg-panel);
    border-bottom: 1px solid var(--rt-border);
    color: var(--rt-fg);
    font-size: var(--rt-size-small);
  }
  .hint.compact {
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    margin: 4px var(--rt-side-inset);
  }
  .mark {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--rt-accent);
  }
  p {
    flex: 1 1 auto;
    min-width: 0;
    margin: 0;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  button {
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .act {
    padding: 2px 8px;
    background: var(--rt-bg);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
  }
  .act:hover {
    background: var(--rt-bg-hover);
  }
  .close {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: var(--rt-radius);
    color: var(--rt-fg-muted);
  }
  .close:hover {
    background: var(--rt-bg-hover);
    color: var(--rt-fg);
  }
  button:focus-visible {
    outline: 2px solid var(--rt-focus);
    outline-offset: 1px;
  }
</style>

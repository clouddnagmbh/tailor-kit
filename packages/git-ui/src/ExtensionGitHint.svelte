<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The one hint of the extensions at the top of a Git view (wave 15, strand E): graph, changes,
     conflict, history. At most one, the first not dismissed (`pickHint`); a close button
     dismisses it per device (or per repository, `ExtensionHint.scope`). Same rules and look as
     `ExtensionHints`: a calm line, never modal, never blocking. `target` carries the situation
     (`selected`, `dragging`, `hunks`) the hint may depend on. -->
<script lang="ts">
  import { Banner, Button } from "@tailor-kit/ui";
  import { dismissalKey, extensionGitHints, pickHint } from "./extension-git";
  import type { GitHintTarget } from "./extensions";
  import { getGitHost } from "./host";

  let { target }: { target: GitHintTarget | null } = $props();

  const host = getGitHost();
  const provided = host.extensions;
  const hintStore = host.hints;
  const dismissed = hintStore.dismissed;
  const shown = $derived(target ? pickHint(extensionGitHints(provided, target), target, $dismissed) : null);

  function dismiss() {
    if (shown && target) hintStore.dismiss(dismissalKey(shown, target));
  }

  function act(action: { run: () => unknown; dismiss?: boolean }) {
    try {
      void Promise.resolve(action.run()).catch((e) => console.warn("git-ui: hint action failed:", e));
    } catch (e) {
      console.warn("git-ui: hint action failed:", e);
    }
    if (action.dismiss) dismiss();
  }
</script>

{#if shown && target}
  {#key shown.key}
    <div class="hint" data-testid="extension-git-hint" data-hint={shown.key} data-view={target.view}>
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
    padding: var(--rt-space-3) var(--rt-space-5) 0;
  }
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

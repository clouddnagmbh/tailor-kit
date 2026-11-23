<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Conflict of one file after a pull, in a modal dialog: our version and theirs side by side,
     "Use ours" / "Use theirs". Resolving changes files, so it is disabled while there are
     unsaved changes (`blocked` says why). Owner: wave 4, strand A2.
     Wave 12, strand B: when the file merges on the model, the conflicted fields come first,
     decided one by one (`merge/MergeFields`); the whole versions stay below. -->
<script lang="ts">
  import type { GitConflict, GitResolution, MergeState } from "../commands/types";
  import MergeFields from "../merge/MergeFields.svelte";
  import { getApp } from "../state/app.svelte";
  import { getGitViews } from "./views.svelte";

  let {
    path,
    blocked,
    onresolve,
    onclose,
  }: {
    path: string;
    blocked: string | null;
    onresolve: (resolution: GitResolution) => void;
    onclose: () => void;
  } = $props();
  const app = getApp();
  const views = getGitViews();

  let conflict = $state<GitConflict | null>(null);
  let error = $state<string | null>(null);
  let merge = $state<MergeState | null>(null);
  const perField = $derived(
    merge !== null &&
      (merge.conflicts.some((c) => c.file === path) || (merge.clean ?? []).includes(path)),
  );

  $effect(() => {
    const p = path;
    conflict = null;
    error = null;
    merge = null;
    // Field conflicts are an addition: without them (or on an error) the file view stays.
    app.backend.merges.conflicts().then(
      (m) => {
        if (p === path) merge = m;
      },
      () => {},
    );
    app.backend.git.conflict(p).then(
      (c) => {
        if (p === path) conflict = c;
      },
      (e: unknown) => {
        if (p === path) error = e instanceof Error ? e.message : String(e);
      },
    );
  });

  function opened(node: HTMLDialogElement) {
    node.showModal();
    return () => node.close();
  }
</script>

<dialog
  data-testid="git-conflict"
  aria-labelledby="rt-git-conflict-title"
  {@attach opened}
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
>
  <header>
    <h2 id="rt-git-conflict-title">Conflict in <code>{path}</code></h2>
    <button onclick={onclose}>Close</button>
  </header>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !conflict}
    <p class="hint">Loading…</p>
  {:else}
    {#if perField && merge}
      <p class="hint">Both sides changed these fields. Pick a side for each; the rest of the file is merged.</p>
      <div class="fields">
        <MergeFields {merge} only={path} {blocked} onresolved={() => onclose()} />
      </div>
      <h3 class="whole">Or keep one whole version</h3>
    {:else}
      <p class="hint">
        Both sides changed this file. Keep one version, or
        <button data-testid="git-resolve-by-hand" onclick={() => { views.open({ kind: "text-merge", path }); onclose(); }}>resolve it line by line</button>.
      </p>
      {#if blocked}<p class="warn" role="status">{blocked}</p>{/if}
    {/if}
    <div class="sides">
      <section aria-labelledby="rt-git-ours">
        <div class="side-head">
          <h3 id="rt-git-ours">Ours (this branch)</h3>
          <button data-testid="git-use-ours" disabled={!!blocked} onclick={() => onresolve("ours")}>Use ours</button>
        </div>
        <pre data-testid="git-conflict-ours">{conflict.ours ?? "(deleted on this side)"}</pre>
      </section>
      <section aria-labelledby="rt-git-theirs">
        <div class="side-head">
          <h3 id="rt-git-theirs">Theirs (incoming)</h3>
          <button data-testid="git-use-theirs" disabled={!!blocked} onclick={() => onresolve("theirs")}>
            Use theirs
          </button>
        </div>
        <pre data-testid="git-conflict-theirs">{conflict.theirs ?? "(deleted on their side)"}</pre>
      </section>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    width: min(1100px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    padding: 0;
    color: var(--rt-fg);
    background: var(--rt-bg);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    box-shadow: var(--rt-shadow);
    font: inherit;
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  dialog::backdrop {
    background: var(--rt-bg-overlay);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--rt-gap);
    padding: var(--rt-gap) calc(var(--rt-gap) * 2);
    border-bottom: 1px solid var(--rt-border);
    background: var(--rt-bg-panel);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: inherit;
  }
  h3 {
    flex: 1;
    margin: 0;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  .fields {
    padding: 0 calc(var(--rt-gap) * 2) calc(var(--rt-gap) * 2);
    overflow: auto;
  }
  .whole {
    padding: 0 calc(var(--rt-gap) * 2) var(--rt-gap);
  }
  .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--rt-gap);
    padding: 0 calc(var(--rt-gap) * 2) calc(var(--rt-gap) * 2);
    min-height: 0;
    overflow: auto;
  }
  section {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .side-head {
    display: flex;
    align-items: center;
    gap: var(--rt-gap);
    margin-bottom: 4px;
  }
  pre {
    margin: 0;
    padding: var(--rt-gap);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg-panel);
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .hint,
  .warn,
  .error {
    margin: 0;
    padding: var(--rt-gap) calc(var(--rt-gap) * 2);
  }
  .hint {
    color: var(--rt-fg-muted);
  }
  .warn {
    color: var(--rt-warn);
  }
  .error {
    color: var(--rt-danger);
  }
</style>

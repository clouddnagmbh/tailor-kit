<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Diff of one file (HEAD → working tree) in a modal dialog: hunks with added and removed
     lines coloured, "Binary file" without hunks. Escape or Close closes it. Owner: wave 4,
     strand A2. -->
<script lang="ts">
  import type { GitDiff } from "./types";
  import { getGitHost } from "./host";
  import { hunkHeader } from "./format";

  let { path, onclose }: { path: string; onclose: () => void } = $props();
  const host = getGitHost();

  let diff = $state<GitDiff | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const p = path;
    diff = null;
    error = null;
    host.backend.git.diff(p).then(
      (d) => {
        if (p === path) diff = d;
      },
      (e: unknown) => {
        if (p === path) error = e instanceof Error ? e.message : String(e);
      },
    );
  });

  const SIGN = { context: " ", added: "+", removed: "-" } as const;

  function opened(node: HTMLDialogElement) {
    node.showModal();
    return () => node.close();
  }
</script>

<dialog
  data-testid="git-diff"
  aria-labelledby="rt-git-diff-title"
  {@attach opened}
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
>
  <header>
    <h2 id="rt-git-diff-title" data-testid="git-diff-path">{path}</h2>
    <button data-testid="git-diff-close" onclick={onclose}>Close</button>
  </header>
  <div class="body">
    {#if error}
      <p class="error" role="alert">{error}</p>
    {:else if !diff}
      <p class="hint">Loading…</p>
    {:else if diff.binary}
      <p class="hint" data-testid="git-diff-binary">Binary file: no line diff.</p>
    {:else if diff.hunks.length === 0}
      <p class="hint" data-testid="git-diff-empty">No changes.</p>
    {:else}
      {#each diff.hunks as hunk, i (i)}
        <section class="hunk" data-testid="git-hunk" aria-label={hunkHeader(hunk)}>
          <div class="hunk-header">{hunkHeader(hunk)}</div>
          {#each hunk.lines as line, k (k)}
            <div class="line {line.kind}" data-kind={line.kind} data-testid="git-line"><span class="sign">{SIGN[line.kind]}</span><span class="text">{line.text}</span></div>
          {/each}
        </section>
      {/each}
    {/if}
  </div>
</dialog>

<style>
  dialog {
    width: min(960px, calc(100vw - 32px));
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
    font-family: var(--rt-font-mono);
    overflow-wrap: anywhere;
  }
  .body {
    overflow: auto;
    padding: var(--rt-gap) 0;
  }
  .hunk {
    margin-bottom: var(--rt-gap);
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
  }
  .hunk-header {
    padding: 2px calc(var(--rt-gap) * 2);
    color: var(--rt-fg-muted);
    background: var(--rt-bg-panel);
  }
  .line {
    padding: 0 calc(var(--rt-gap) * 2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .line.added {
    background: color-mix(in srgb, var(--rt-ok) 16%, transparent);
  }
  .line.removed {
    background: color-mix(in srgb, var(--rt-danger) 16%, transparent);
  }
  .line.added .sign {
    color: var(--rt-ok);
  }
  .line.removed .sign {
    color: var(--rt-danger);
  }
  .sign {
    display: inline-block;
    width: 1.5ch;
    user-select: none;
  }
  .hint,
  .error {
    margin: 0;
    padding: var(--rt-gap) calc(var(--rt-gap) * 2);
  }
  .hint {
    color: var(--rt-fg-muted);
  }
  .error {
    color: var(--rt-danger);
  }
</style>

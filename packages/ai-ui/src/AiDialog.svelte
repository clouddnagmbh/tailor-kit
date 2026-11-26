<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The frame of the AI assistant's dialogs (wave 8, strand E): modal, Escape closes unless
     busy. The content brings its own buttons. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    testid,
    busy = false,
    onclose,
    children,
  }: { title: string; testid: string; busy?: boolean; onclose: () => void; children: Snippet } = $props();

  function opened(node: HTMLDialogElement) {
    node.showModal();
    return () => node.close();
  }
  const titleId = $derived(`${testid}-title`);
</script>

<dialog
  class="ai"
  data-testid={testid}
  aria-labelledby={titleId}
  aria-busy={busy}
  {@attach opened}
  oncancel={(e) => {
    e.preventDefault();
    if (!busy) onclose();
  }}
>
  <header><h2 id={titleId}>{title}</h2></header>
  <div class="body">{@render children()}</div>
</dialog>

<style>
  dialog {
    width: min(640px, calc(100vw - 32px));
    padding: 0;
    color: var(--rt-fg);
    background: var(--rt-bg);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    font: inherit;
  }
  dialog::backdrop {
    background: var(--rt-bg-overlay);
  }
  header {
    padding: var(--rt-gap) calc(var(--rt-gap) * 2);
    background: var(--rt-bg-panel);
  }
  h2 {
    margin: 0;
    font-size: 1em;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--rt-gap);
    padding: calc(var(--rt-gap) * 2);
    max-height: 75vh;
    overflow: auto;
  }
  .body :global(p) {
    margin: 0;
  }
  .body :global(.muted) {
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .body :global(.error) {
    color: var(--rt-danger);
  }
  .body :global(.row) {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-gap);
  }
  .body :global(label.field) {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  .body :global(input),
  .body :global(select),
  .body :global(textarea) {
    font: inherit;
    color: var(--rt-fg);
    background: var(--rt-bg);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    padding: 4px 6px;
  }
  .body :global(textarea) {
    min-height: 5em;
    resize: vertical;
  }
  .body :global(button) {
    font: inherit;
    color: var(--rt-fg);
    background: var(--rt-bg);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    padding: 2px var(--rt-gap);
    cursor: pointer;
  }
  .body :global(button:disabled) {
    opacity: 0.6;
    cursor: default;
  }
  .body :global(button.primary) {
    color: var(--rt-accent-fg);
    background: var(--rt-accent);
    border-color: var(--rt-accent);
  }
  .body :global(pre) {
    margin: 0;
    padding: 6px 8px;
    overflow: auto;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg-panel);
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15, strand D: the 3-way view of a conflicted file that is not a collection file. Where both
     sides changed the same lines, each block shows the common ancestor, ours and theirs, and you
     pick one (or both); the result below is editable. "Resolve" sends the text to GitApi.resolveText.
     Resolving changes files, so it waits for unsaved changes to be saved (the backend says so too). -->
<script lang="ts">
  import { BackendError } from "../api";
  import type { GitConflict } from "../types";
  import type { Component } from "svelte";

  import type { ExtensionMergeEditorProps } from "../extensions";
  import { extensionFileEditors } from "../extension-git";
  import { getGitExtensions } from "../host";
  import ExtensionGitActions from "../ExtensionGitActions.svelte";
  import ExtensionGitHint from "../ExtensionGitHint.svelte";
  import { getGitHost } from "../host";
  import { Badge, Banner, Button, EmptyState, TextArea } from "@tailor-kit/ui";
  import { chooseEditor, chosenEditor, pickEditor } from "../extension/editor-choice";
  import EditorPicker from "../extension/EditorPicker.svelte";
  import { diff3, join, toLines, type Block, type Choice } from "./diff3";

  let { path, onclose }: { path: string; onclose: () => void } = $props();
  const host = getGitHost();
  const provided = getGitExtensions();

  // A merge editor of an extension can take the place of the view below (`resolve` and `close`:
  // see `ExtensionMergeEditorProps`). The host closes once, however often the editor asks.
  const editors = $derived(extensionFileEditors(provided, "merge", path));
  let choice = $state(chosenEditor(host.storage, "merge"));
  const active = $derived(pickEditor(editors, choice));
  const Editor = $derived(active ? (active.action.component as unknown as Component<ExtensionMergeEditorProps>) : null);
  let closed = false;
  function closeOnce() {
    if (closed) return;
    closed = true;
    onclose();
  }
  /** What an extension's editor calls: the same steps as the view's own "Resolve", errors go
   *  back to the editor (it stays open). */
  async function resolveWith(text: string): Promise<void> {
    if (host.unsaved().length > 0) throw new Error("Save your changes first: resolving rewrites the file on disk.");
    await host.backend.git.resolveText(path, text);
    await host.reload();
    closeOnce();
  }

  let conflict = $state<GitConflict | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let choices = $state<Choice[]>([]);
  /** The text the user typed over the generated result; `null` while it follows the choices. */
  let edited = $state<string | null>(null);

  const blocks = $derived<Block[]>(conflict ? diff3(toLines(conflict.base), toLines(conflict.ours), toLines(conflict.theirs)) : []);
  const conflicts = $derived(blocks.filter((b) => b.kind === "conflict"));
  const generated = $derived(join(blocks, choices));
  const result = $derived(edited ?? generated);
  const blocked = $derived(host.unsaved().length > 0 ? "Save your changes first: resolving rewrites the file on disk." : null);

  $effect(() => {
    const p = path;
    conflict = null;
    error = null;
    edited = null;
    closed = false;
    host.backend.git.conflict(p).then(
      (c) => {
        if (p !== path) return;
        conflict = c;
        choices = diff3(toLines(c.base), toLines(c.ours), toLines(c.theirs))
          .filter((b) => b.kind === "conflict")
          .map(() => "ours" as Choice);
      },
      (e: unknown) => {
        if (p === path) error = e instanceof Error ? e.message : String(e);
      },
    );
  });

  function choose(index: number, choice: Choice) {
    choices[index] = choice;
    edited = null;
  }

  async function resolve() {
    busy = true;
    error = null;
    try {
      await host.backend.git.resolveText(path, result);
      await host.reload();
      closeOnce();
    } catch (e) {
      error = e instanceof BackendError || e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  const lines = (l: string[]) => (l.length === 0 ? "(nothing)" : l.join("\n"));
</script>

<div class="merge" data-testid="text-merge">
  <ExtensionGitHint target={{ place: "git", repository: host.activeRepository ?? "", view: "conflict" }} />
  <header>
    <h2>Resolve <code>{path}</code></h2>
    <EditorPicker {editors} kind="merge" bind:value={choice} onchange={(v) => chooseEditor(host.storage, "merge", v)} />
    <ExtensionGitActions target={{ kind: "conflict", repository: host.activeRepository ?? "", path }} label="Conflict actions" />
    <Button variant="ghost" onclick={onclose}>Back</Button>
  </header>
  {#if error}
    <Banner tone="danger"><span data-testid="text-merge-error">{error}</span></Banner>
  {/if}
  {#if Editor && conflict && provided}
    <Editor ctx={provided.ctx} repository={host.activeRepository ?? ""} {conflict} resolve={resolveWith} close={closeOnce} />
  {:else if !conflict && !error}
    <p class="hint">Loading…</p>
  {:else if conflict}
    {#if conflicts.length === 0}
      <EmptyState icon="merge" title="No overlapping changes" text="Both sides changed different lines, so the merged result below is ready." />
    {:else}
      <p class="hint" data-testid="text-merge-count">
        {conflicts.length} {conflicts.length === 1 ? "place needs" : "places need"} your decision. Pick a version for each; you can edit the result before you resolve.
      </p>
    {/if}
    {#each blocks as block, at (at)}
      {#if block.kind === "conflict"}
        {@const index = blocks.slice(0, at).filter((b) => b.kind === "conflict").length}
        <section class="block" data-testid="text-merge-block" aria-label="Conflict {index + 1}">
          <div class="head">
            <Badge kind="label" tone="warn" value="Conflict {index + 1}" />
            <div class="picks" role="group" aria-label="Keep for conflict {index + 1}">
              <Button size="sm" variant={choices[index] === "ours" ? "primary" : "secondary"} data-testid="text-merge-ours" onclick={() => choose(index, "ours")}>Mine</Button>
              <Button size="sm" variant={choices[index] === "theirs" ? "primary" : "secondary"} data-testid="text-merge-theirs" onclick={() => choose(index, "theirs")}>Theirs</Button>
              <Button size="sm" variant={choices[index] === "both" ? "primary" : "secondary"} data-testid="text-merge-both" onclick={() => choose(index, "both")}>Both</Button>
              <Button size="sm" variant={choices[index] === "base" ? "primary" : "secondary"} data-testid="text-merge-base" onclick={() => choose(index, "base")}>Original</Button>
            </div>
          </div>
          <div class="sides">
            <div><h3>Original</h3><pre>{lines(block.base)}</pre></div>
            <div><h3>Mine (this branch)</h3><pre data-testid="text-merge-mine">{lines(block.ours)}</pre></div>
            <div><h3>Theirs (incoming)</h3><pre data-testid="text-merge-their">{lines(block.theirs)}</pre></div>
          </div>
        </section>
      {/if}
    {/each}
    <TextArea label="Result" mono rows={10} data-testid="text-merge-result" value={result} oninput={(e) => (edited = (e.currentTarget as HTMLTextAreaElement).value)} />
    {#if blocked}<Banner tone="warn">{blocked}</Banner>{/if}
    <footer>
      <Button variant="primary" data-testid="text-merge-resolve" disabled={busy || !!blocked} loading={busy} onclick={resolve}>Resolve with this result</Button>
      <Button variant="ghost" onclick={onclose}>Cancel</Button>
    </footer>
  {/if}
</div>

<style>
  .merge {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
    padding: var(--rt-space-4);
    overflow: auto;
    min-width: 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: var(--rt-text-md);
    overflow-wrap: anywhere;
  }
  h3 {
    margin: 0 0 var(--rt-space-1);
    font-size: var(--rt-text-sm);
    color: var(--rt-fg-muted);
  }
  .hint {
    margin: 0;
    color: var(--rt-fg-muted);
  }
  .block {
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-md);
    background: var(--rt-surface-1);
    padding: var(--rt-space-3);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--rt-space-3);
    margin-bottom: var(--rt-space-2);
    flex-wrap: wrap;
  }
  .picks {
    display: flex;
    gap: var(--rt-space-1);
  }
  .sides {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--rt-space-2);
  }
  pre {
    margin: 0;
    padding: var(--rt-space-2);
    border-radius: var(--rt-radius-sm);
    background: var(--rt-surface-sunken);
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-sm);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  footer {
    display: flex;
    gap: var(--rt-space-2);
  }
</style>

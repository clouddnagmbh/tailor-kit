<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A file's diff in the app's own view, or in an editor an extension offers for it (wave 15,
     strand E, `ExtensionFileEditor` of kind `diff`). The own view is the default; the choice is
     a small control above it, remembered per device. `children` is the own view. Without an
     editor for the file nothing changes: `children` only. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { ExtensionDiffEditorProps } from "../../extensions";
  import { extensionFileEditors } from "../../shell/extension-git";
  import { getExtensions } from "../../shell/extension-tabs";
  import { getApp } from "../../state/app.svelte";
  import { chooseEditor, chosenEditor, pickEditor } from "./editor-choice";
  import EditorPicker from "./EditorPicker.svelte";

  let {
    path,
    from,
    to,
    children,
  }: {
    /** Repository-relative with a leading `/`. */
    path: string;
    from: string;
    /** `null`: the working tree. */
    to: string | null;
    children: Snippet;
  } = $props();

  const provided = getExtensions();
  const app = getApp();
  const editors = $derived(extensionFileEditors(provided, "diff", path));
  let choice = $state(chosenEditor("diff"));
  const active = $derived(pickEditor(editors, choice));
  const Editor = $derived(active ? (active.action.component as unknown as import("svelte").Component<ExtensionDiffEditorProps>) : null);
</script>

{#if editors.length > 0}
  <div class="switch" data-testid="file-editor-switch">
    <EditorPicker {editors} kind="diff" builtin="Standard" bind:value={choice} onchange={(v) => chooseEditor("diff", v)} />
  </div>
{/if}
{#if Editor && provided}
  <Editor ctx={provided.ctx} repository={app.activeRepository ?? ""} {path} {from} {to} close={() => (choice = "")} />
{:else}
  {@render children()}
{/if}

<style>
  .switch {
    display: flex;
    justify-content: flex-end;
    padding: var(--rt-space-2) var(--rt-space-5);
  }
</style>

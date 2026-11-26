<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The files of the active repository (wave 15, strand B): a tree on the left, the chosen file
     on the right, at any revision (a branch, a tag, a commit) or in the working tree. A file
     changed in the working tree also offers its comparison with `HEAD`. A folder path shows
     only the tree. -->
<script lang="ts">
  import { untrack } from "svelte";

  import type { GitBlob, GitTreeEntry } from "../types";
  import ExtensionGitActions from "../ExtensionGitActions.svelte";
  import { getGitHost } from "../host";
  import FileEditorSwitch from "../extension/FileEditorSwitch.svelte";
  import { Button, EmptyState, SegmentedControl, Spinner, TextField } from "@tailor-kit/ui";
  import BlobView from "../viewers/BlobView.svelte";
  import FileCompare from "../viewers/FileCompare.svelte";
  import FileTree from "./FileTree.svelte";
  import { crumbs, normalizePath, revOf } from "./model";

  let { path, rev }: { path: string; rev?: string } = $props();
  const host = getGitHost();

  // The props say where the view starts; later choices are its own.
  let revision = $state<string | null>(untrack(() => rev ?? null));
  let revText = $state(untrack(() => rev ?? ""));
  let at = $state(untrack(() => normalizePath(path)));
  let kind = $state<GitTreeEntry["kind"] | null>(null);
  let blob = $state<GitBlob | null>(null);
  let error = $state<string | null>(null);
  let compare = $state(false);
  let reload = $state(0);
  /** "Revision…" chosen: the field shows even before a revision is typed. */
  let picking = $state(false);
  let changedState = $state<string | null>(null);
  let load = 0;

  // Another file chosen from outside (a graph or review link) replaces the view's choice.
  let seen = untrack(() => ({ path, rev }));
  $effect(() => {
    if (path !== seen.path || rev !== seen.rev) {
      seen = { path, rev };
      at = normalizePath(path);
      revision = rev ?? null;
      revText = rev ?? "";
      kind = null;
    }
  });

  const isRoot = $derived(at === "/");
  const mode = $derived(picking ? "other" : revision === null ? "worktree" : revision === "HEAD" ? "head" : "other");

  async function open() {
    const token = ++load;
    blob = null;
    error = null;
    compare = false;
    changedState = null;
    if (isRoot || kind === "dir") return;
    try {
      const next = await host.backend.git.blob(revision, at);
      if (token !== load) return;
      blob = next;
      if (revision === null) {
        const dir = at.slice(0, at.lastIndexOf("/")) || "/";
        const list = await host.backend.git.fileTree(null, dir);
        if (token === load) changedState = list.find((e) => e.path === at)?.state ?? null;
      }
    } catch (e) {
      if (token === load) error = e instanceof Error ? e.message : String(e);
    }
  }

  $effect(() => {
    void at;
    void revision;
    void kind;
    void reload;
    void host.reloadCount;
    void open();
  });

  function choose(entry: GitTreeEntry) {
    at = entry.path;
    kind = entry.kind;
  }

  function setMode(value: string) {
    picking = value === "other";
    if (picking) {
      revText = revision ?? "";
      return;
    }
    revision = value === "worktree" ? null : value === "head" ? "HEAD" : revOf(revText) ?? "HEAD";
    revText = revision ?? "";
    kind = null;
  }

  function applyRev() {
    revision = revOf(revText);
    if (revision !== null) picking = false;
    kind = null;
  }

  const folder = $derived(isRoot || kind === "dir");
</script>

<section class="files" data-testid="file-view" aria-label="Files">
  <header class="bar">
    <nav class="crumbs" aria-label="Path" data-testid="file-path">
      <button type="button" class="crumb" onclick={() => ((at = "/"), (kind = "dir"))}>Repository</button>
      {#each crumbs(at) as c (c.path)}
        <span aria-hidden="true">/</span>
        <button type="button" class="crumb" onclick={() => ((at = c.path), (kind = null))}>{c.name}</button>
      {/each}
    </nav>
    <SegmentedControl
      size="sm"
      label="Version"
      value={mode}
      options={[
        { value: "worktree", label: "Working tree", testid: "file-rev-worktree" },
        { value: "head", label: "HEAD", testid: "file-rev-head" },
        { value: "other", label: "Revision…", testid: "file-rev-other" },
      ]}
      onchange={setMode}
    />
    {#if mode === "other"}
      <TextField
        size="sm"
        mono
        placeholder="branch, tag or commit"
        aria-label="Revision"
        bind:value={revText}
        data-testid="file-rev-input"
        onkeydown={(e) => e.key === "Enter" && applyRev()}
        onblur={applyRev}
      />
    {/if}
    <Button size="sm" icon="sync" data-testid="file-reload" onclick={() => reload++}>Reload</Button>
  </header>

  <div class="split">
    <div class="side" data-testid="file-side">
      <FileTree rev={revision} selected={at} onselect={choose} {reload} />
    </div>
    <div class="main" data-testid="file-main">
      {#if folder}
        <EmptyState icon="folder-open" title={isRoot ? "Pick a file" : at} text="Open a folder in the tree and choose a file to read it." size="sm" />
      {:else if error}
        <p class="error" role="alert" data-testid="file-error">{error}</p>
      {:else if !blob}
        <p class="hint"><Spinner size="sm" label="Loading" /> Loading…</p>
      {:else}
        {#if !folder}
          <div class="ext" data-testid="file-extension-actions">
            <ExtensionGitActions target={{ kind: "file", repository: host.activeRepository ?? "", path: at, rev: revision ?? undefined }} label="File actions" />
          </div>
        {/if}
        {#if revision === null && changedState && changedState !== "deleted"}
          <div class="actions">
            <Button size="sm" icon="compare" data-testid="file-compare-head" onclick={() => (compare = !compare)}>{compare ? "Show the file" : "Compare with HEAD"}</Button>
          </div>
        {/if}
        <div class="content">
          {#if compare}
            {#if changedState === "untracked"}
              <FileCompare before={null} after={{ rev: null, path: at }} {reload} />
            {:else}
              <FileEditorSwitch path={at} from="HEAD" to={null}>
                <FileCompare before={{ rev: "HEAD", path: at }} after={{ rev: null, path: at }} {reload} />
              </FileEditorSwitch>
            {/if}
          {:else}
            <BlobView {blob} />
          {/if}
        </div>
      {/if}
    </div>
  </div>
</section>

<style>
  .files {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-4);
    padding: var(--rt-space-4) var(--rt-space-5);
    border-bottom: 1px solid var(--rt-border);
  }
  .crumbs {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-2);
    min-width: 0;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
  }
  .crumb {
    padding: 0 var(--rt-space-2);
    border: 0;
    border-radius: var(--rt-radius-sm);
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .crumb:hover {
    background: var(--rt-surface-sunken);
  }
  .crumb:focus-visible {
    outline: none;
    box-shadow: var(--rt-focus-ring);
  }
  .split {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) minmax(0, 3fr);
    flex: 1;
    min-height: 0;
  }
  .side {
    overflow: auto;
    border-right: 1px solid var(--rt-border);
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .ext {
    display: flex;
    justify-content: flex-end;
    padding: var(--rt-space-3) var(--rt-space-5) 0;
  }
  .ext:not(:has(*)) {
    display: none;
  }
  .actions {
    padding: var(--rt-space-3) var(--rt-space-5);
    border-bottom: 1px solid var(--rt-border);
  }
  .content {
    flex: 1;
    overflow: auto;
    min-height: 0;
  }
  .hint,
  .error {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    margin: 0;
    padding: var(--rt-space-5);
  }
  .hint {
    color: var(--rt-fg-muted);
  }
  .error {
    color: var(--rt-danger-fg);
  }
</style>

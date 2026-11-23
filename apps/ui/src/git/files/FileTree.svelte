<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The files of the repository as a tree (wave 15, strand B): folders are read when they open
     (`git.fileTree`), at a revision or in the working tree, where changed files carry their
     state and ignored ones are dimmed. -->
<script lang="ts">
  import { untrack } from "svelte";

  import type { GitTreeEntry } from "../../commands/types";
  import { getApp } from "../../state/app.svelte";
  import { Badge, Icon, Spinner } from "../../ui";
  import { stateLetter } from "./model";

  let {
    rev,
    selected,
    onselect,
    reload = 0,
  }: {
    /** `null`: the working tree. */
    rev: string | null;
    /** The path shown on the right; its folders are opened. */
    selected: string | null;
    onselect: (entry: GitTreeEntry) => void;
    reload?: number;
  } = $props();
  const app = getApp();

  let entries = $state<Record<string, GitTreeEntry[]>>({});
  let open = $state<Record<string, boolean>>({ "/": true });
  let errors = $state<Record<string, string>>({});
  let loading = $state<Record<string, boolean>>({});
  let generation = 0;

  async function read(dir: string) {
    const at = generation;
    loading[dir] = true;
    try {
      const list = await app.backend.git.fileTree(rev, dir);
      if (at !== generation) return;
      entries[dir] = list;
      delete errors[dir];
    } catch (e) {
      if (at === generation) errors[dir] = e instanceof Error ? e.message : String(e);
    } finally {
      if (at === generation) loading[dir] = false;
    }
  }

  // A new revision or a reload starts over; open folders are read again.
  $effect(() => {
    void rev;
    void reload;
    // Written here and read by `read`: untracked, or the effect would trigger itself.
    untrack(() => {
      generation++;
      entries = {};
      errors = {};
      loading = {};
      for (const dir of Object.keys(open)) if (open[dir]) void read(dir);
    });
  });

  // The folders above the selected path stay open.
  $effect(() => {
    if (!selected) return;
    const parts = selected.split("/").filter(Boolean);
    untrack(() => {
      for (let i = 1; i < parts.length; i++) {
        const dir = `/${parts.slice(0, i).join("/")}`;
        if (!open[dir]) {
          open[dir] = true;
          void read(dir);
        }
      }
    });
  });

  function toggle(entry: GitTreeEntry) {
    if (entry.kind === "dir") {
      const next = !open[entry.path];
      open[entry.path] = next;
      if (next && entries[entry.path] === undefined) void read(entry.path);
    }
    onselect(entry);
  }

  const TONE = { A: "ok", M: "warn", D: "danger", "?": "neutral" } as const;
</script>

{#snippet level(dir: string, depth: number)}
  {#if errors[dir]}
    <p class="error" role="alert" style:padding-left="{depth * 14 + 12}px">{errors[dir]}</p>
  {:else if entries[dir] === undefined}
    <p class="hint" style:padding-left="{depth * 14 + 12}px"><Spinner size="sm" label="Loading" /></p>
  {:else}
    {#each entries[dir] as entry (entry.path)}
      {@const letter = stateLetter(entry.state)}
      <button
        type="button"
        class="entry"
        class:selected={entry.path === selected}
        class:ignored={entry.ignored}
        role="treeitem"
        aria-selected={entry.path === selected}
        aria-expanded={entry.kind === "dir" ? Boolean(open[entry.path]) : undefined}
        style:padding-left="{depth * 14 + 8}px"
        data-testid="tree-entry"
        data-path={entry.path}
        data-kind={entry.kind}
        data-state={entry.state ?? ""}
        onclick={() => toggle(entry)}
      >
        {#if entry.kind === "dir"}
          <Icon name={open[entry.path] ? "chevron-down" : "chevron"} size="sm" />
          <Icon name={open[entry.path] ? "folder-open" : "folder"} size="sm" />
        {:else}
          <span class="gap"></span>
          <Icon name={entry.kind === "submodule" ? "link" : "file-text"} size="sm" />
        {/if}
        <span class="name">{entry.name}</span>
        {#if entry.kind === "submodule"}<span class="note">submodule</span>{/if}
        {#if letter}<Badge kind="status" value={letter} tone={TONE[letter]} />{/if}
        {#if entry.ignored}<span class="note">ignored</span>{/if}
      </button>
      {#if entry.kind === "dir" && open[entry.path]}
        {@render level(entry.path, depth + 1)}
      {/if}
    {/each}
    {#if entries[dir].length === 0}
      <p class="hint" style:padding-left="{depth * 14 + 12}px">Empty folder</p>
    {/if}
  {/if}
{/snippet}

<div class="tree" role="tree" aria-label="Files" data-testid="file-tree">
  {@render level("/", 0)}
</div>

<style>
  .tree {
    padding: var(--rt-space-3) 0;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    width: 100%;
    padding: var(--rt-space-2) var(--rt-space-4);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .entry:hover {
    background: var(--rt-surface-sunken);
  }
  .entry.selected {
    background: var(--rt-accent-bg, var(--rt-surface-3));
  }
  .entry:focus-visible {
    outline: none;
    box-shadow: var(--rt-focus-ring);
  }
  .entry.ignored {
    opacity: 0.55;
  }
  .gap {
    width: var(--rt-space-5);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note,
  .hint {
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
  }
  .hint,
  .error {
    margin: 0;
    padding: var(--rt-space-2) var(--rt-space-4);
  }
  .error {
    color: var(--rt-danger-fg);
  }
</style>

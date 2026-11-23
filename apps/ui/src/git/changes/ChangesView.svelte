<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The working copy of the active repository (wave 15, strand B): every changed file of the
     whole repository, not only the collection, grouped by folder, with a filter. A click shows
     the change: collection files field by field (or as lines), text with the words that
     changed marked, images side by side, binary files and Git LFS pointers as facts. Files an
     ignore rule covers never appear. The default commit stays "without staging" (the Git
     panel); this view reads. `j`/`k` and the arrows move, Enter opens the file. -->
<script lang="ts">
  import type { CollectionDiff, GitWorkingCopy, GitWorkingFile } from "../../commands/types";
  import SemanticDiff from "../../diff/SemanticDiff.svelte";
  import { getApp } from "../../state/app.svelte";
  import { Badge, Banner, Button, Chip, EmptyState, Icon, IconButton, SegmentedControl, Spinner, TextField } from "../../ui";
  import { getGitViews } from "../views.svelte";
  import FileCompare from "../viewers/FileCompare.svelte";
  import type { Side } from "../viewers/model";
  import { countStates, fileName, filterFiles, groupByFolder, NO_FILTER, statusLetter, summaryText, visibleFiles, type ChangeFilter, type StateFilter } from "./model";

  const app = getApp();
  const views = getGitViews();

  let copy = $state<GitWorkingCopy | null>(null);
  let error = $state<string | null>(null);
  let filter = $state<ChangeFilter>({ ...NO_FILTER });
  let selected = $state<string | null>(null);
  let mode = $state("fields");
  let diff = $state<CollectionDiff | null>(null);
  let reload = $state(0);
  let load = 0;

  async function refresh() {
    const token = ++load;
    try {
      const next = await app.backend.git.workingCopy();
      if (token !== load) return;
      copy = next;
      error = null;
      if (selected !== null && !next.files.some((f) => f.path === selected)) selected = null;
      reload++;
    } catch (e) {
      if (token === load) error = e instanceof Error ? e.message : String(e);
    }
  }

  // Saved edits, a reload and the active repository change the working copy.
  $effect(() => {
    void app.loadCount;
    void app.dirty.length;
    void app.activeRepository;
    void refresh();
  });

  const files = $derived(copy ? visibleFiles(copy.files) : []);
  const counts = $derived(countStates(files));
  const shown = $derived(filterFiles(files, filter));
  const groups = $derived(groupByFolder(shown));
  const current = $derived(files.find((f) => f.path === selected) ?? null);

  const STATE_FILTERS: { value: StateFilter; label: string; count: (c: typeof counts) => number }[] = [
    { value: "all", label: "All", count: (c) => c.all },
    { value: "modified", label: "Modified", count: (c) => c.modified },
    { value: "untracked", label: "New", count: (c) => c.untracked },
    { value: "deleted", label: "Deleted", count: (c) => c.deleted },
    { value: "conflicted", label: "Conflicts", count: (c) => c.conflicted },
  ];

  const TONE = { A: "ok", M: "warn", D: "danger", R: "info", U: "danger", "?": "neutral" } as const;

  /** What a file looked like at `HEAD` and what it is now. */
  function sides(file: GitWorkingFile): { before: Side | null; after: Side | null } {
    const before = file.state === "untracked" && !file.from ? null : { rev: "HEAD", path: file.from ?? file.path };
    const after = file.state === "deleted" ? null : { rev: null, path: file.path };
    return { before, after };
  }
  const pair = $derived(current ? sides(current) : null);

  // The field-by-field diff of the active collection's saved changes, for collection files.
  const collectionFile = $derived(current?.collection ? current : null);
  const semantic = $derived(collectionFile !== null && collectionFile.collection === app.activeCollection);
  $effect(() => {
    void reload;
    diff = null;
    if (!semantic) return;
    let live = true;
    app.backend.diffs.working().then(
      (d) => {
        if (live) diff = d;
      },
      () => {
        if (live) diff = null;
      },
    );
    return () => {
      live = false;
    };
  });
  const fileDiff = $derived.by((): CollectionDiff | null => {
    if (!diff || !current) return null;
    const mine = (file: string) => current.path === `/${file}` || current.path.endsWith(`/${file}`);
    return { ...diff, changes: diff.changes.filter((c) => mine(c.file)), files: diff.files.filter((f) => mine(f.path)) };
  });
  const fieldMode = $derived(semantic && mode === "fields" && fileDiff !== null && fileDiff.changes.length > 0);

  function move(by: number) {
    if (shown.length === 0) return;
    const order = groups.flatMap((g) => g.files);
    const at = order.findIndex((f) => f.path === selected);
    selected = order[Math.min(order.length - 1, Math.max(0, at + by))]!.path;
  }

  function onkeydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t?.closest("input, textarea, select, [role='radiogroup']")) return;
    if (e.key === "ArrowDown" || e.key === "j") move(1);
    else if (e.key === "ArrowUp" || e.key === "k") move(-1);
    else if (e.key === "Enter" && current) views.open({ kind: "file", path: current.path });
    else return;
    e.preventDefault();
  }

  const branchLabel = $derived(copy ? (copy.branch ?? (copy.detached ? "detached HEAD" : "no commits yet")) : "");
  const sync = $derived(copy && (copy.ahead > 0 || copy.behind > 0) ? `↑${copy.ahead} ↓${copy.behind}` : "");
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="changes" data-testid="changes-view" aria-label="Changes in the working copy" {onkeydown}>
  <header class="bar">
    {#if copy}
      <span data-testid="changes-branch"><Chip label={branchLabel} icon="branch" mono /></span>
      {#if sync}<span class="muted" data-testid="changes-sync">{sync}</span>{/if}
      {#if copy.operation}<Badge value={copy.operation.replace("-", " ")} tone="warn" data-testid="changes-operation" />{/if}
      <span class="summary" data-testid="changes-summary">{summaryText(counts)}</span>
    {/if}
    <span class="grow"></span>
    <Button size="sm" icon="folder-open" data-testid="changes-browse" onclick={() => views.open({ kind: "file", path: "/" })}>Browse files</Button>
    <IconButton icon="sync" size="sm" label="Refresh" data-testid="changes-refresh" onclick={() => void refresh()} />
  </header>

  {#if error}
    <p class="error" role="alert" data-testid="changes-error">{error}</p>
  {:else if !copy}
    <p class="hint"><Spinner size="sm" label="Loading" /> Reading the working copy…</p>
  {:else}
    {#if copy.truncated}
      <Banner tone="warn" title="Only the first {copy.files.length} changed files are listed" data-testid="changes-truncated">
        Something changed nearly every file (a checkout or a formatter). Park or discard the changes in the Git panel, or filter below.
      </Banner>
    {/if}
    <div class="filters">
      <TextField size="sm" icon="search" placeholder="Filter files" aria-label="Filter files" bind:value={filter.text} data-testid="changes-filter" />
      {#each STATE_FILTERS as s (s.value)}
        {@const n = s.count(counts)}
        {#if s.value === "all" || n > 0}
          <Chip size="sm" label="{s.label} {n}" selected={filter.state === s.value} data-testid="changes-state-{s.value}" onclick={() => (filter.state = s.value)} />
        {/if}
      {/each}
      <Chip size="sm" label="Staged" selected={filter.staged} data-testid="changes-staged" onclick={() => (filter.staged = !filter.staged)} />
    </div>

    {#if files.length === 0}
      <div data-testid="changes-clean"><EmptyState icon="check" title="The working copy is clean" text="Nothing differs from the last commit. Ignored files are not listed." /></div>
    {:else}
      <div class="split">
        <div class="list" role="listbox" aria-label="Changed files" data-testid="changes-list">
          {#each groups as g (g.folder)}
            <div class="folder" data-testid="changes-folder"><Icon name="folder" size="sm" /> {g.folder}</div>
            {#each g.files as f (f.path)}
              {@const letter = statusLetter(f)}
              <button
                type="button"
                role="option"
                aria-selected={f.path === selected}
                class="row"
                class:selected={f.path === selected}
                data-testid="changes-row"
                data-path={f.path}
                data-state={f.state}
                onclick={() => (selected = f.path)}
                ondblclick={() => views.open({ kind: "file", path: f.path })}
              >
                <Badge kind="status" value={letter} tone={TONE[letter]} />
                <span class="name">{fileName(f.path)}</span>
                {#if f.from}<span class="from" data-testid="changes-from">← {f.from}</span>{/if}
                {#if f.staged}<span class="staged" title="In the index">staged</span>{/if}
                {#if f.collection}<span class="tag" title="A collection file: shown field by field">API</span>{/if}
              </button>
            {/each}
          {/each}
          {#if shown.length === 0}
            <p class="hint" data-testid="changes-none">No file matches the filter.</p>
          {/if}
        </div>

        <div class="detail" data-testid="changes-detail">
          {#if current && pair}
            <div class="detail-bar">
              <span class="path" data-testid="changes-path">{current.path}</span>
              {#if semantic}
                <SegmentedControl
                  size="sm"
                  label="Diff style"
                  options={[
                    { value: "fields", label: "By field", testid: "changes-mode-fields" },
                    { value: "lines", label: "Lines", testid: "changes-mode-lines" },
                  ]}
                  bind:value={mode}
                />
              {:else if collectionFile?.collection}
                <Button size="sm" data-testid="changes-open-collection" onclick={() => void app.activate(collectionFile.collection!)}>Open its collection for the field view</Button>
              {/if}
              <Button size="sm" icon="file-text" data-testid="changes-open" onclick={() => views.open({ kind: "file", path: current.path })}>Open file</Button>
            </div>
            <div class="viewer">
              {#if fieldMode && fileDiff}
                <SemanticDiff diff={fileDiff} />
              {:else}
                <FileCompare before={pair.before} after={pair.after} {reload} />
              {/if}
            </div>
          {:else}
            <EmptyState icon="compare" title="Pick a file" text="Its change shows here: words that changed are marked, images side by side." size="sm" />
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</section>

<style>
  .changes {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .bar,
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-4);
    padding: var(--rt-space-4) var(--rt-space-5);
    border-bottom: 1px solid var(--rt-border);
  }
  .grow {
    flex: 1;
  }
  .muted {
    color: var(--rt-fg-muted);
  }
  .summary {
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
  }
  .split {
    display: grid;
    grid-template-columns: minmax(240px, 2fr) minmax(0, 5fr);
    flex: 1;
    min-height: 0;
  }
  .list {
    overflow: auto;
    border-right: 1px solid var(--rt-border);
    padding: var(--rt-space-3) 0;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    padding: var(--rt-space-3) var(--rt-space-5);
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
    font-family: var(--rt-font-mono);
    content-visibility: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    width: 100%;
    padding: var(--rt-space-2) var(--rt-space-5) var(--rt-space-2) var(--rt-space-7);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    content-visibility: auto;
    contain-intrinsic-size: auto 28px;
  }
  .row:hover {
    background: var(--rt-surface-sunken);
  }
  .row.selected {
    background: var(--rt-accent-bg, var(--rt-surface-3));
  }
  .row:focus-visible {
    outline: none;
    box-shadow: var(--rt-focus-ring);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .from,
  .staged,
  .tag {
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
    white-space: nowrap;
  }
  .from {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .detail-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-4);
    padding: var(--rt-space-4) var(--rt-space-5);
    border-bottom: 1px solid var(--rt-border);
  }
  .path {
    flex: 1;
    min-width: 0;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
    overflow-wrap: anywhere;
  }
  .viewer {
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

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15, strand C: every commit that changed a file, any file of the repository, following
     renames (the entry names the path at that commit and where it came from). Pick a commit to
     see what it changed in the file; from there: blame the file at that commit, show the commit in
     the history, or bring this version of the file back (the restore wizard of strand D plans it,
     keeps a backup and can be undone). -->
<script lang="ts">
  import type { GitDiff, GitFileCommit } from "../../commands/types";
  import { getApp } from "../../state/app.svelte";
  import { Avatar, Badge, Button, EmptyState, Skeleton } from "../../ui";
  import FileDiff from "../graph/FileDiff.svelte";
  import { relativeTime, shortId } from "../graph/model";
  import { getGitViews } from "../views.svelte";
  import { openGitWizard } from "../wizards/open";

  let { path }: { path: string } = $props();

  const app = getApp();
  const views = getGitViews();
  /** Entries asked for at a time. */
  const PAGE = 100;

  const STATUS = { added: "A", modified: "M", deleted: "D", renamed: "R" } as const;

  let entries = $state<GitFileCommit[] | null>(null);
  let more = $state(false);
  let limit = $state(PAGE);
  let error = $state<string | null>(null);
  let chosen = $state<string | null>(null);
  let diff = $state<GitDiff | null>(null);
  let diffError = $state<string | null>(null);
  let now = $state(Date.now());
  let token = 0;

  const entry = $derived(entries?.find((e) => e.commit.oid === chosen) ?? null);
  const name = $derived(path.replace(/^\//, "").split("/").pop() ?? path);

  async function load(file: string, count: number) {
    const mine = ++token;
    error = null;
    try {
      const list = await app.backend.git.fileLog(file, count + 1);
      if (mine !== token) return;
      more = list.length > count;
      entries = list.slice(0, count);
      if (!chosen || !entries.some((e) => e.commit.oid === chosen)) chosen = entries[0]?.commit.oid ?? null;
    } catch (e) {
      if (mine !== token) return;
      error = e instanceof Error ? e.message : String(e);
      entries = [];
    }
  }

  let key = "";
  $effect(() => {
    const k = `${app.activeCollection}\0${path}`;
    if (k === key) return;
    key = k;
    limit = PAGE;
    chosen = null;
    entries = null;
    void load(path, PAGE);
  });

  $effect(() => {
    const e = entry;
    diff = null;
    diffError = null;
    if (!e) return;
    const mine = e.commit.oid;
    app.backend.git
      .fileDiff(null, e.commit.oid, e.path)
      .then((d) => {
        if (chosen === mine) diff = d;
      })
      .catch((err: unknown) => {
        if (chosen === mine) diffError = err instanceof Error ? err.message : String(err);
      });
  });

  function onkey(e: KeyboardEvent) {
    if (!entries || entries.length === 0) return;
    const i = entries.findIndex((x) => x.commit.oid === chosen);
    const next = e.key === "ArrowDown" || e.key === "j" ? i + 1 : e.key === "ArrowUp" || e.key === "k" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? entries.length - 1 : null;
    if (next === null) return;
    e.preventDefault();
    chosen = entries[Math.max(0, Math.min(entries.length - 1, next))]!.commit.oid;
    (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-oid="${chosen}"]`)?.focus();
  }

  async function restore(e: GitFileCommit) {
    await openGitWizard(app, { kind: "restore-files", preset: { rev: e.commit.oid, paths: [e.path.startsWith("/") ? e.path : `/${e.path}`] } });
    await load(path, limit);
  }

  function more_() {
    limit += PAGE;
    void load(path, limit);
  }
</script>

<div class="history" data-testid="file-history" data-path={path}>
  <div class="toolbar">
    <span class="file" title={path}>{name}</span>
    {#if entries}<span class="count" data-testid="file-history-count">{entries.length}{more ? "+" : ""} commit{entries.length === 1 && !more ? "" : "s"}</span>{/if}
    <span class="spacer"></span>
    <Button size="sm" variant="ghost" icon="file-text" data-testid="file-history-blame" onclick={() => views.open({ kind: "blame", path })}>Blame</Button>
  </div>

  {#if error}
    <EmptyState icon="history" title="No history to show" text={error} data-testid="file-history-error" />
  {:else if !entries}
    <div class="loading"><Skeleton lines={6} label="Reading the history of the file" /></div>
  {:else if entries.length === 0}
    <EmptyState icon="history" title="Not in the history" text="No commit of the current branch has this file. A file that was never committed has no history yet." data-testid="file-history-empty" />
  {:else}
    <div class="body">
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul class="list" role="listbox" aria-label="Commits that changed the file, newest first" tabindex="-1" onkeydown={onkey} data-testid="file-history-list">
        {#each entries as e (e.commit.oid)}
          <li>
            <button
              type="button"
              role="option"
              aria-selected={e.commit.oid === chosen}
              class="entry"
              class:chosen={e.commit.oid === chosen}
              data-testid="file-history-entry"
              data-oid={e.commit.oid}
              tabindex={e.commit.oid === chosen ? 0 : -1}
              onclick={() => (chosen = e.commit.oid)}
            >
              <Badge kind="status" value={STATUS[e.change]} />
              <Avatar name={e.commit.author} />
              <span class="what">
                <span class="summary">{e.commit.summary}</span>
                <span class="meta">
                  {e.commit.author}, {relativeTime(e.commit.time, now)}
                  {#if e.from}<span class="rename" data-testid="file-history-rename"> · renamed from {e.from.replace(/^\//, "")}</span>{/if}
                  {#if e.path.replace(/^\//, "") !== path.replace(/^\//, "") && !e.from} · as {e.path.replace(/^\//, "")}{/if}
                </span>
              </span>
              <code class="id">{shortId(e.commit.oid)}</code>
            </button>
          </li>
        {/each}
        {#if more}
          <li class="more"><Button size="sm" variant="ghost" data-testid="file-history-more" onclick={more_}>Show older commits</Button></li>
        {/if}
      </ul>

      <section class="detail" aria-label="What the commit changed in the file">
        {#if entry}
          <div class="actions">
            <Button size="sm" variant="secondary" icon="file-text" data-testid="file-history-blame-at" onclick={() => views.open({ kind: "blame", path: entry.path.startsWith("/") ? entry.path : `/${entry.path}`, rev: entry.commit.oid })}>Blame here</Button>
            <Button size="sm" variant="secondary" icon="graph" data-testid="file-history-show" onclick={() => views.open({ kind: "graph", select: entry.commit.oid })}>Show in the history</Button>
            {#if entry.change !== "deleted"}
              <Button size="sm" variant="secondary" icon="undo" data-testid="file-history-restore" onclick={() => void restore(entry)}>Restore this version</Button>
            {/if}
          </div>
          <FileDiff {diff} error={diffError} />
        {/if}
      </section>
    </div>
  {/if}
</div>

<svelte:window onfocus={() => (now = Date.now())} />

<style>
  .history {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    flex: none;
    padding: var(--rt-space-3) var(--rt-space-6);
    border-bottom: 1px solid var(--rt-border);
  }
  .file {
    font-family: var(--rt-font-mono);
    font-weight: var(--rt-weight-semibold);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .count {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .loading {
    padding: var(--rt-space-6);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(280px, 40%) minmax(0, 1fr);
  }
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
    overflow: auto;
    border-right: 1px solid var(--rt-border);
  }
  .entry {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    width: 100%;
    min-height: var(--rt-control-lg);
    padding: var(--rt-space-3) var(--rt-space-6);
    border: 0;
    background: none;
    color: var(--rt-fg);
    font: var(--rt-font-body);
    text-align: left;
    cursor: pointer;
  }
  .entry:hover {
    background: var(--rt-bg-hover);
  }
  .entry.chosen {
    background: var(--rt-bg-selected);
  }
  .entry:focus-visible {
    outline: none;
    box-shadow: var(--rt-focus-ring);
  }
  .what {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .summary {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .id {
    flex: none;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .more {
    padding: var(--rt-space-4) var(--rt-space-6);
  }
  .detail {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
    min-width: 0;
    min-height: 0;
    overflow: auto;
    padding: var(--rt-space-5) var(--rt-space-6);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-3);
  }
</style>

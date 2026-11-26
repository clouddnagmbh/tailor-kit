<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15, strand C: who last changed each line of any file, as of a revision. Lines that came
     from one commit form a band with the commit on its first line (summary, author, age, id).
     "Blame before this change" goes one commit back for that hunk; the id opens the commit in
     the history; the toolbar goes to the file's history. A binary file has no lines. -->
<script lang="ts">
  import type { GitBlame } from "../../commands/types";
  import ExtensionGitActions from "../../shell/ExtensionGitActions.svelte";
  import ExtensionGitHint from "../../shell/ExtensionGitHint.svelte";
  import { getGitHost } from "../host";
  import { Avatar, Button, EmptyState, IconButton, Skeleton, tooltip } from "../../ui";
  import { absoluteTime, relativeTime, shortId } from "../graph/model";
  import { getGitViews } from "../views.svelte";
  import { blameLines, commitCount, textLines } from "./model";

  let { path, rev }: { path: string; rev?: string } = $props();

  const host = getGitHost();
  const views = getGitViews();

  /** The revisions blamed so far, so "back" returns to the one before. */
  let trail = $state<string[]>([]);
  let current = $state<string | undefined>(undefined);
  let blame = $state<GitBlame | null>(null);
  let lines = $state<string[]>([]);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let now = $state(Date.now());
  let token = 0;

  const rows = $derived(blame ? blameLines(blame, lines) : []);
  const at = $derived(current ?? "HEAD");
  const name = $derived(path.replace(/^\//, "").split("/").pop() ?? path);

  async function load(target: string | undefined, file: string) {
    const mine = ++token;
    loading = true;
    error = null;
    try {
      const git = host.backend.git;
      const b = await git.blame(target ?? "HEAD", file);
      const text = b.binary ? "" : ((await git.show(b.rev, b.path)) ?? "");
      if (mine !== token) return;
      blame = b;
      lines = textLines(text);
    } catch (e) {
      if (mine !== token) return;
      blame = null;
      error = e instanceof Error ? e.message : String(e);
    } finally {
      if (mine === token) loading = false;
    }
  }

  // The view opens at `rev`; a new `path` or `rev` (a link from another view) starts over.
  let key = "";
  $effect(() => {
    const k = `${host.cacheScope}\0${path}\0${rev ?? ""}`;
    if (k === key) return;
    key = k;
    trail = [];
    current = rev;
    void load(rev, path);
  });

  async function before(oid: string) {
    // The commit before this change: its first parent.
    try {
      const info = await host.backend.git.commitInfo(oid);
      const parent = info.parents[0];
      if (!parent) {
        error = "This is the first commit of the file: there is nothing before it.";
        return;
      }
      trail = [...trail, current ?? "HEAD"];
      current = parent;
      await load(parent, blame?.hunks.find((h) => h.commit.oid === oid)?.path ?? path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function back() {
    const previous = trail[trail.length - 1];
    if (previous === undefined) return;
    trail = trail.slice(0, -1);
    current = previous === "HEAD" ? rev : previous;
    await load(current, path);
  }
</script>

<div class="blame" data-testid="blame-view" data-path={path}>
  <ExtensionGitHint target={{ place: "git", repository: host.activeRepository ?? "", view: "history" }} />
  <div class="toolbar">
    <IconButton icon="chevron-left" size="sm" label="Back to the later version" disabled={trail.length === 0} onclick={() => void back()} data-testid="blame-back" />
    <span class="file" title={path}>{name}</span>
    <span class="at" data-testid="blame-at">at {at === "HEAD" ? "the latest commit" : shortId(at)}</span>
    {#if blame && !blame.binary}<span class="count">{blame.lines.toLocaleString("en-US")} lines from {commitCount(blame)} commit{commitCount(blame) === 1 ? "" : "s"}</span>{/if}
    <span class="spacer"></span>
    <ExtensionGitActions target={{ kind: "file", repository: host.activeRepository ?? "", path: blame?.path ?? path, rev: at === "HEAD" ? undefined : at }} label="File actions" />
    <Button size="sm" variant="ghost" icon="history" data-testid="blame-file-history" onclick={() => views.open({ kind: "file-history", path: blame?.path ?? path })}>File history</Button>
  </div>

  {#if loading && !blame}
    <div class="loading"><Skeleton lines={6} label="Reading the history of the file" /></div>
  {:else if error && !blame}
    <EmptyState icon="file" title="Nothing to blame" text={error} data-testid="blame-error" />
  {:else if blame?.binary}
    <EmptyState icon="file" title="Binary file" text="A binary file has no lines to blame. Its history lists the commits that changed it." data-testid="blame-binary">
      {#snippet actions()}<Button size="sm" onclick={() => views.open({ kind: "file-history", path: blame?.path ?? path })}>Show the file history</Button>{/snippet}
    </EmptyState>
  {:else if blame}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if rows.length === 0}
      <EmptyState icon="file-text" title="An empty file" text="There are no lines to blame." />
    {:else}
      <div class="lines" role="table" aria-label="Who changed each line" aria-busy={loading} data-testid="blame-lines">
        {#each rows as r (r.n)}
          <div class="row band-{r.band}" class:first={r.first} role="row" data-testid="blame-line" data-line={r.n} data-commit={r.hunk.commit.oid}>
            <div class="gutter" role="cell">
              {#if r.first}
                <Avatar name={r.hunk.commit.author} />
                <button type="button" class="summary" data-testid="blame-commit" title="{r.hunk.commit.summary}, {absoluteTime(r.hunk.commit.time)}" onclick={() => views.open({ kind: "graph", select: r.hunk.commit.oid })}>{r.hunk.commit.summary}</button>
                <span class="when" use:tooltip={`${r.hunk.commit.author}, ${absoluteTime(r.hunk.commit.time)}`}>{relativeTime(r.hunk.commit.time, now)}</span>
                <code class="id">{shortId(r.hunk.commit.oid)}</code>
                <IconButton icon="history" size="sm" label="Blame before this change" data-testid="blame-before" onclick={() => void before(r.hunk.commit.oid)} />
              {/if}
            </div>
            <span class="num" role="cell">{r.n}</span>
            <span class="text" role="cell"><code>{r.text}</code></span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<svelte:window onfocus={() => (now = Date.now())} />

<style>
  .blame {
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
    min-width: 0;
  }
  .file {
    font-family: var(--rt-font-mono);
    font-weight: var(--rt-weight-semibold);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .at,
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
  .error {
    margin: var(--rt-space-4) var(--rt-space-6);
    color: var(--rt-danger-fg);
    font: var(--rt-font-small);
  }
  .lines {
    flex: 1;
    min-height: 0;
    overflow: auto;
    font: var(--rt-font-small);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(220px, 34%) 48px minmax(0, 1fr);
    align-items: stretch;
    min-height: 20px;
    content-visibility: auto;
    contain-intrinsic-size: auto 20px;
  }
  .row.band-1 {
    background: var(--rt-surface-sunken);
  }
  .row.first {
    border-top: 1px solid var(--rt-border);
  }
  .gutter {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    padding: 0 var(--rt-space-4) 0 var(--rt-space-6);
    min-width: 0;
    border-right: 1px solid var(--rt-border);
  }
  .summary {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--rt-fg);
    font: inherit;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
  }
  .summary:hover {
    color: var(--rt-accent);
    text-decoration: underline;
  }
  .when,
  .id {
    flex: none;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    white-space: nowrap;
  }
  .num {
    padding: 0 var(--rt-space-4);
    color: var(--rt-fg-subtle);
    text-align: right;
    font-variant-numeric: tabular-nums;
    line-height: 20px;
  }
  .text {
    padding: 0 var(--rt-space-4);
    white-space: pre;
    line-height: 20px;
  }
  .text code {
    font-family: var(--rt-font-mono);
  }
</style>

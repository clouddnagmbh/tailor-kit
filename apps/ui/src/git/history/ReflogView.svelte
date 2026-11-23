<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15, strand C: "Lost work". The reflog records where HEAD or a branch pointed, also after
     commands outside the app; a commit no branch, tag or remote branch reaches any more is marked
     "Not on any branch". "Bring it back" opens the branch wizard (strand D, `restore-branch`): it
     shows what will happen, keeps a backup and can be undone. Nothing here moves a ref itself. -->
<script lang="ts">
  import type { GitRef, GitReflogEntry } from "../../commands/types";
  import { getApp } from "../../state/app.svelte";
  import { Avatar, Badge, Button, EmptyState, Select, Skeleton } from "../../ui";
  import { relativeTime, shortId } from "../graph/model";
  import { getGitViews } from "../views.svelte";
  import { openGitWizard } from "../wizards/open";
  import { recoveredName, reflogRows, type ReflogRow } from "./model";

  let { name }: { name?: string } = $props();

  const app = getApp();
  const views = getGitViews();
  /** Entries read, and commits of the history looked at to tell lost from kept. */
  const LIMIT = 200;
  const GRAPH = 5000;

  let chosen = $state("HEAD");
  let refs = $state<GitRef[]>([]);
  let rows = $state<ReflogRow[] | null>(null);
  let error = $state<string | null>(null);
  let now = $state(Date.now());
  let token = 0;

  const choices = $derived([
    { value: "HEAD", label: "Where you have been (HEAD)" },
    ...refs.filter((r) => r.kind === "local").map((r) => ({ value: r.full, label: r.name })),
  ]);
  const lostCount = $derived(rows?.filter((r) => r.lost === true).length ?? 0);

  async function load(ref: string) {
    const mine = ++token;
    error = null;
    try {
      const git = app.backend.git;
      const [entries, page] = await Promise.all([git.reflog(ref, LIMIT), git.graph({ skip: 0, limit: GRAPH })]);
      if (mine !== token) return;
      refs = page.refs.length > 0 ? page.refs : refs;
      const reachable = new Set(page.commits.map((c) => c.oid));
      rows = reflogRows(entries, reachable, !page.more);
    } catch (e) {
      if (mine !== token) return;
      error = e instanceof Error ? e.message : String(e);
      rows = [];
    }
  }

  let key = "";
  $effect(() => {
    const k = `${app.activeCollection}\0${name ?? ""}`;
    if (k === key) return;
    key = k;
    chosen = name ?? "HEAD";
    rows = null;
    void load(chosen);
  });

  function choose(value: string) {
    chosen = value;
    rows = null;
    void load(value);
  }

  async function bringBack(entry: GitReflogEntry) {
    await openGitWizard(app, { kind: "restore-branch", preset: { name: recoveredName(entry), target: entry.new } });
    await load(chosen);
  }

  function describe(e: GitReflogEntry): string {
    return e.message || "Moved";
  }
</script>

<div class="reflog" data-testid="reflog-view" data-ref={chosen}>
  <div class="toolbar">
    <Select label="Look at" size="sm" value={chosen} options={choices} onchange={(e) => choose((e.currentTarget as HTMLSelectElement).value)} data-testid="reflog-ref" />
    {#if rows}<span class="count" data-testid="reflog-count">{rows.length} step{rows.length === 1 ? "" : "s"}{lostCount > 0 ? `, ${lostCount} not on any branch` : ""}</span>{/if}
  </div>
  <p class="intro">Git remembers where {chosen === "HEAD" ? "you have been" : "this branch pointed"}, also after a reset or a deleted branch. Work that no branch reaches any more is marked; bring it back as a branch and nothing is lost.</p>

  {#if error}
    <EmptyState icon="undo" title="Nothing to show" text={error} data-testid="reflog-error" />
  {:else if !rows}
    <div class="loading"><Skeleton lines={6} label="Reading the reflog" /></div>
  {:else if rows.length === 0}
    <EmptyState icon="undo" title="No steps recorded" text="Git has not recorded anything for this yet. It starts with the first commit." data-testid="reflog-empty" />
  {:else}
    <ul class="list" aria-label="Steps, newest first" data-testid="reflog-list">
      {#each rows as r (r.entry.index)}
        {@const e = r.entry}
        <li class="entry" class:lost={r.lost === true} data-testid="reflog-entry" data-oid={e.new} data-lost={r.lost === null ? "unknown" : String(r.lost)}>
          <Avatar name={e.commit?.author ?? "?"} />
          <span class="what">
            <span class="summary">{e.commit?.summary ?? "A commit that is no longer in the repository"}</span>
            <span class="meta">{describe(e)} · {relativeTime(e.time, now)}</span>
          </span>
          {#if r.lost === true}<Badge value="Not on any branch" tone="warn" data-testid="reflog-lost" />{/if}
          <code class="id">{shortId(e.new)}</code>
          <span class="buttons">
            {#if e.commit}
              <Button size="sm" variant="ghost" data-testid="reflog-show" onclick={() => views.open({ kind: "graph", select: e.new })} disabled={r.lost === true}>Show</Button>
              <Button size="sm" variant={r.lost === true ? "secondary" : "ghost"} icon="undo" data-testid="reflog-bring-back" onclick={() => void bringBack(e)}>Bring it back</Button>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<svelte:window onfocus={() => (now = Date.now())} />

<style>
  .reflog {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    align-items: end;
    gap: var(--rt-space-5);
    flex: none;
    padding: var(--rt-space-4) var(--rt-space-6) 0;
  }
  .count {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    padding-bottom: var(--rt-space-2);
  }
  .intro {
    margin: 0;
    padding: var(--rt-space-4) var(--rt-space-6);
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    border-bottom: 1px solid var(--rt-border);
  }
  .loading {
    padding: var(--rt-space-6);
  }
  .list {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow: auto;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    min-height: var(--rt-control-lg);
    padding: var(--rt-space-3) var(--rt-space-6);
    border-bottom: 1px solid var(--rt-border);
  }
  .entry.lost {
    background: var(--rt-warn-bg);
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
  .buttons {
    display: flex;
    gap: var(--rt-space-2);
    flex: none;
  }
</style>

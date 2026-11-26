<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15, strand C: search the whole history of the repository by message, author, path and
     content (and a date range); every field you fill has to match. A hit shows what matched; a
     click opens the commit in the history, the paths of a hit open its blame or file history.
     Content searches read the history in pieces ("Search further") because they open files. -->
<script lang="ts">
  import type { GitHistoryHit, GitHistoryQuery } from "../../commands/types";
  import ExtensionGitActions from "../../shell/ExtensionGitActions.svelte";
  import ExtensionGitHint from "../../shell/ExtensionGitHint.svelte";
  import { getGitHost } from "../host";
  import { Avatar, Badge, Button, EmptyState, IconButton, Skeleton, TextField } from "../../ui";
  import { relativeTime, shortId } from "../graph/model";
  import { getGitViews } from "../views.svelte";
  import { EMPTY_FIELDS, fieldsOf, MATCH_LABEL, toQuery, type SearchFields } from "./model";

  let { query }: { query?: Partial<GitHistoryQuery> } = $props();

  const host = getGitHost();
  const views = getGitViews();
  /** Hits asked for at a time. */
  const PAGE = 50;

  let fields = $state<SearchFields>({ ...EMPTY_FIELDS });
  let hits = $state<GitHistoryHit[]>([]);
  let more = $state(false);
  let scanned = $state(0);
  let asked = $state<Omit<GitHistoryQuery, "limit" | "skip"> | null>(null);
  let searching = $state(false);
  let error = $state<string | null>(null);
  let now = $state(Date.now());
  let token = 0;

  const filled = $derived(toQuery(fields) !== null);
  const content = $derived(Boolean(asked?.content));

  async function run(next: Omit<GitHistoryQuery, "limit" | "skip">, skip: number) {
    const mine = ++token;
    searching = true;
    error = null;
    try {
      const page = await host.backend.git.searchHistory({ ...next, skip, limit: PAGE });
      if (mine !== token) return;
      hits = skip === 0 ? page.hits : [...hits, ...page.hits];
      more = page.more;
      scanned = skip === 0 ? page.scanned : scanned + page.scanned;
    } catch (e) {
      if (mine !== token) return;
      error = e instanceof Error ? e.message : String(e);
      if (skip === 0) hits = [];
      more = false;
    } finally {
      if (mine === token) searching = false;
    }
  }

  function search() {
    const q = toQuery(fields);
    if (!q) return;
    asked = q;
    void run(q, 0);
  }

  function further() {
    if (asked) void run(asked, hits.length);
  }

  // The view opens with a query (from the graph's search, or a link): run it.
  let key = "";
  $effect(() => {
    const k = `${host.cacheScope}\0${JSON.stringify(query ?? {})}`;
    if (k === key) return;
    key = k;
    fields = { ...EMPTY_FIELDS, ...fieldsOf(query) };
    asked = null;
    hits = [];
    more = false;
    error = null;
    if (toQuery(fields)) search();
  });
</script>

<div class="search" data-testid="history-search">
  <ExtensionGitHint target={{ place: "git", repository: host.activeRepository ?? "", view: "history" }} />
  <form class="form" onsubmit={(e) => (e.preventDefault(), search())}>
    <TextField label="Message" size="sm" placeholder="Words in the commit message" bind:value={fields.message} data-testid="search-message" />
    <TextField label="Author" size="sm" placeholder="Name or email" bind:value={fields.author} data-testid="search-author" />
    <TextField label="Path" size="sm" placeholder="/folder or /file.yml" bind:value={fields.path} data-testid="search-path" />
    <TextField label="Content" size="sm" placeholder="Text added or removed in a file" bind:value={fields.content} data-testid="search-content" />
    <TextField label="From" size="sm" type="date" bind:value={fields.since} data-testid="search-since" />
    <TextField label="To" size="sm" type="date" bind:value={fields.until} data-testid="search-until" />
    <div class="go">
      <Button type="submit" variant="primary" size="sm" icon="search" disabled={!filled || searching} loading={searching && hits.length === 0} data-testid="search-run">Search</Button>
    </div>
  </form>

  <div class="results" aria-busy={searching} data-testid="search-results">
    {#if error}
      <p class="error" role="alert" data-testid="search-error">{error}</p>
    {/if}
    {#if !asked}
      <EmptyState icon="search" title="Search the history" text="Fill in one or more fields: every field has to match. Content finds commits that added or removed a line with the text." data-testid="search-intro" />
    {:else if searching && hits.length === 0}
      <div class="loading"><Skeleton lines={5} label="Searching the history" /></div>
    {:else if hits.length === 0 && !error}
      <EmptyState icon="search" title="No commit matches" text={more ? "" : "Try fewer fields or a shorter text. Searching is not case-sensitive."} data-testid="search-none" />
    {:else}
      <p class="summary" data-testid="search-count">{hits.length}{more ? "+" : ""} commit{hits.length === 1 && !more ? "" : "s"}</p>
      <ul class="hits">
        {#each hits as h (h.commit.oid)}
          <li class="hit" data-testid="search-hit" data-oid={h.commit.oid}>
            <button type="button" class="main" onclick={() => views.open({ kind: "graph", select: h.commit.oid })}>
              <Avatar name={h.commit.author} />
              <span class="what">
                <span class="summary-line">{h.commit.summary}</span>
                <span class="meta">{h.commit.author}, {relativeTime(h.commit.time, now)}</span>
              </span>
              <span class="matched">{#each h.matched as m (m)}<Badge value={MATCH_LABEL[m]} tone="accent" />{/each}</span>
              <code class="id">{shortId(h.commit.oid)}</code>
            </button>
            <div class="ext" data-testid="search-hit-actions">
              <ExtensionGitActions target={{ kind: "commit", repository: host.activeRepository ?? "", commit: h.commit.oid }} label="Commit actions" />
            </div>
            {#if h.paths && h.paths.length > 0}
              <ul class="paths">
                {#each h.paths as p (p)}
                  <li>
                    <code>{p.replace(/^\//, "")}</code>
                    <IconButton icon="file-text" size="sm" label="Blame {p.replace(/^\//, "")} here" onclick={() => views.open({ kind: "blame", path: p, rev: h.commit.oid })} />
                    <IconButton icon="history" size="sm" label="History of {p.replace(/^\//, "")}" onclick={() => views.open({ kind: "file-history", path: p })} />
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
      {#if more}
        <div class="further">
          <Button size="sm" variant="secondary" loading={searching} data-testid="search-more" onclick={further}>{content ? "Search further" : "Show more"}</Button>
          {#if content}<span class="note">A content search opens files, so it reads the history in pieces. {scanned.toLocaleString("en-US")} commits read so far.</span>{/if}
        </div>
      {/if}
    {/if}
  </div>
</div>

<svelte:window onfocus={() => (now = Date.now())} />

<style>
  .search {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .form {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: var(--rt-space-4);
    align-items: end;
    flex: none;
    padding: var(--rt-space-5) var(--rt-space-6);
    border-bottom: 1px solid var(--rt-border);
  }
  .go {
    display: flex;
  }
  .results {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--rt-space-4) var(--rt-space-6);
  }
  .loading {
    padding: var(--rt-space-4) 0;
  }
  .error {
    margin: 0 0 var(--rt-space-4);
    color: var(--rt-danger-fg);
    font: var(--rt-font-small);
  }
  .summary {
    margin: 0 0 var(--rt-space-3);
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .hits,
  .paths {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .hit {
    border-bottom: 1px solid var(--rt-border);
  }
  .main {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    width: 100%;
    min-height: var(--rt-control-lg);
    padding: var(--rt-space-3) 0;
    border: 0;
    background: none;
    color: var(--rt-fg);
    font: var(--rt-font-body);
    text-align: left;
    cursor: pointer;
  }
  .main:hover .summary-line {
    color: var(--rt-accent);
  }
  .main:focus-visible {
    outline: none;
    box-shadow: var(--rt-focus-ring);
  }
  .what {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .summary-line {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .matched {
    display: flex;
    gap: var(--rt-space-2);
  }
  .id {
    flex: none;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .paths {
    padding: 0 0 var(--rt-space-3) 36px;
  }
  .paths li {
    display: flex;
    align-items: center;
    gap: var(--rt-space-2);
    font: var(--rt-font-small);
  }
  .further {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    padding: var(--rt-space-5) 0;
  }
  .note {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .ext {
    display: flex;
    justify-content: flex-end;
    padding: 0 var(--rt-space-4) var(--rt-space-2);
  }
  .ext:not(:has(*)) {
    display: none;
  }
</style>

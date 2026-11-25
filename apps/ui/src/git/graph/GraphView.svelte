<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 14, strand F: the commit graph of the active collection's repository (all branches,
     remote branches, tags, HEAD) as coloured lanes (`--rt-lane-*`, `layout.ts`), virtualised
     (only the rows in view exist; pages of 2000 commits load while scrolling), with the commit
     details (`CommitDetails.svelte`) and context menus that open the wizards of strand G
     (`../wizards/open.ts`) and the compare and diff views.

     Keys (the list is a listbox): ↑/↓ or j/k move, PageUp/PageDown, Home/End, Enter goes to the
     changed files, Space marks the parent to compare with, the context-menu key or Shift+F10
     opens the menu, `/` searches (Enter/Shift+Enter: next/previous match, Escape: back to the
     list). Mod+click marks a second commit. Any repository: nothing here assumes the repository
     is a collection. -->
<script lang="ts">
  import { onMount, tick } from "svelte";

  import type { GitGraphCommit } from "../../commands/types";
  import type { GitActionTarget, GitHintTarget } from "../../extensions";
  import { extensionRangeActions } from "../../shell/extension-actions";
  import { gitActionEntries, withRefActions } from "../../shell/extension-git";
  import { getExtensions } from "../../shell/extension-tabs";
  import ExtensionActions from "../../shell/ExtensionActions.svelte";
  import ExtensionGitHint from "../../shell/ExtensionGitHint.svelte";
  import { getApp } from "../../state/app.svelte";
  import { Button, EmptyState, IconButton, Menu, SegmentedControl, Skeleton, TextField, type MenuEntry } from "../../ui";
  import { getGitViews } from "../views.svelte";
  import { toMenu, type ActionHost } from "./actions";
  import CommitDetails from "./CommitDetails.svelte";
  import { GraphData, type GraphScope } from "./graph.svelte";
  import GraphRow from "./GraphRow.svelte";
  import { commitMenu, matches, refLabels, selectTarget, type GraphContext } from "./model";

  let { select }: { select?: string } = $props();

  const app = getApp();
  const views = getGitViews();
  const data = new GraphData(() => app.backend.git);

  /** Row height and lane width, px. */
  const ROW = 28;
  const LANE = 14;
  /** Lanes drawn at most; wider histories are cut at the right edge of the graph column. */
  const MAX_LANES = 16;
  const OVERSCAN = 4;
  /** Load the next page when this few rows are left below the view. */
  const AHEAD = 300;

  let scope = $state<GraphScope>("all");
  let selected = $state<string | null>(null);
  let base = $state<string | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let list = $state<HTMLElement>();
  let search = $state<HTMLInputElement>();
  let query = $state("");
  let menu = $state<{ items: MenuEntry[]; anchor: HTMLElement | { x: number; y: number }; label: string } | null>(null);
  let menuOpen = $state(false);
  let now = $state(Date.now());
  let notice = $state("");
  /** A commit row is being dragged (extensions' hints may offer what to do with it). */
  let dragging = $state(false);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  const n = $derived(data.commits.length);
  const selectedIndex = $derived(selected ? (data.index.get(selected) ?? -1) : -1);
  const selectedCommit = $derived(selectedIndex >= 0 ? data.commits[selectedIndex]! : null);
  // Wave 16: actions of extensions on the two commits selected together (base and selection).
  const extensions = getExtensions();
  const rangeActions = $derived(
    base && selected && base !== selected
      ? extensionRangeActions(extensions, { collection: app.activeCollection, from: base, to: selected, source: "graph" })
      : [],
  );
  const hintTarget = $derived<GitHintTarget>({
    place: "git",
    repository: app.activeRepository ?? "",
    view: "graph",
    selected: !selected ? 0 : base && base !== selected ? 2 : 1,
    dragging,
  });
  const baseCommit = $derived(base ? (data.commits[data.index.get(base) ?? -1] ?? null) : null);
  const ctx: GraphContext = $derived({ head: data.head, branch: data.branch });
  const lanes = $derived(Math.min(MAX_LANES, Math.max(2, data.maxWidth)));
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const last = $derived(Math.min(n, Math.ceil((scrollTop + viewport) / ROW) + OVERSCAN));
  // Rows are recycled: row `i` always lives in slot `i % pool`, so scrolling re-targets mounted
  // rows (props change) instead of destroying and mounting them. The pool is larger than the
  // window, so slots never collide; the array is in slot order, so the DOM never moves nodes.
  const pool = $derived(Math.ceil(viewport / ROW) + 2 * OVERSCAN + 8);
  const visible = $derived(
    Array.from({ length: Math.max(0, last - first) }, (_, k) => first + k).sort((a, b) => (a % pool) - (b % pool)),
  );
  const hits = $derived(query.trim() ? data.commits.flatMap((c, i) => (matches(c, data.byFull, query) ? [i] : [])) : []);
  const hitSet = $derived(new Set(hits));
  const hitPos = $derived(hits.indexOf(selectedIndex));
  const hexQuery = $derived(/^[0-9a-f]{4,40}$/i.test(query.trim()));

  const host: ActionHost = {
    app,
    views,
    reload: () => void reload(),
    select: (rev) => void goTo(rev),
    notify: (text) => {
      notice = text;
      clearTimeout(noticeTimer);
      noticeTimer = setTimeout(() => (notice = ""), 2500);
    },
  };

  async function reload(): Promise<void> {
    const keep = selected;
    await data.reload(scope);
    if (base && !data.index.has(base)) base = null;
    if (keep && data.index.has(keep)) return;
    selected = data.head && data.index.has(data.head) ? data.head : (data.commits[0]?.oid ?? null);
  }

  /** Selects `rev` (a commit id or prefix, or a ref name), loading pages until it is there. */
  async function goTo(rev: string, focus = true): Promise<boolean> {
    const target = selectTarget(rev, data.refs) ?? rev;
    const i = await data.find(target);
    if (i < 0) {
      host.notify?.(`${rev} is not in this history`);
      return false;
    }
    selected = data.commits[i]!.oid;
    await tick();
    reveal(i, true);
    if (focus) list?.focus({ preventScroll: true });
    return true;
  }

  /** Scrolls row `i` into view (`center`: into the middle, for jumps). */
  function reveal(i: number, center = false) {
    if (!list) return;
    const top = i * ROW;
    if (center && (top < list.scrollTop || top + ROW > list.scrollTop + viewport)) {
      list.scrollTop = Math.max(0, top - viewport / 2 + ROW / 2);
    } else if (top < list.scrollTop) list.scrollTop = top;
    else if (top + ROW > list.scrollTop + viewport) list.scrollTop = top + ROW - viewport;
    scrollTop = list.scrollTop;
  }

  function move(to: number) {
    if (n === 0) return;
    const i = Math.max(0, Math.min(n - 1, to));
    selected = data.commits[i]!.oid;
    reveal(i);
    if (i > n - AHEAD) void data.loadMore();
  }

  // The active collection's repository: load again when it changes.
  let loadedFor: string | null | undefined;
  $effect(() => {
    const id = app.activeCollection;
    if (id === loadedFor) return;
    loadedFor = id;
    base = null;
    void (async () => {
      await data.reload(scope);
      if (select && (await goTo(select, false))) return;
      selected = data.head && data.index.has(data.head) ? data.head : (data.commits[0]?.oid ?? null);
    })();
  });

  // A new `select` while the view is open (a link from another view).
  let lastSelect: string | undefined | null = null;
  $effect(() => {
    const s = select;
    if (lastSelect === null || s === lastSelect) {
      lastSelect = s;
      return;
    }
    lastSelect = s;
    if (s && data.ready) void goTo(s, false);
  });

  $effect(() => {
    if (last > n - AHEAD && data.more && !data.loading) void data.loadMore();
  });

  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 60_000);
    return () => {
      clearInterval(t);
      clearTimeout(noticeTimer);
    };
  });

  function setScope(value: string) {
    scope = value as GraphScope;
    void reload();
  }

  function pick(i: number, e: MouseEvent) {
    const oid = data.commits[i]!.oid;
    list?.focus({ preventScroll: true });
    if ((e.metaKey || e.ctrlKey) && selected && selected !== oid) {
      // The older of the two (further down) is where the comparison starts.
      if (i > selectedIndex) base = oid;
      else {
        base = selected;
        selected = oid;
      }
      return;
    }
    if (e.metaKey || e.ctrlKey) {
      base = null;
      return;
    }
    selected = oid;
  }

  function openMenu(commit: GitGraphCommit, anchor: HTMLElement | { x: number; y: number }) {
    const repository = app.activeRepository ?? "";
    const fail = (message: string) => (app.error = message);
    // The extensions' actions on this commit (or on the two marked commits) close the menu,
    // those on a branch or tag sit in that label's submenu.
    const target: GitActionTarget =
      base && base !== commit.oid && selected === commit.oid
        ? { kind: "commits", repository, commits: [commit.oid, base] }
        : { kind: "commit", repository, commit: commit.oid };
    const own = withRefActions(extensions, repository, toMenu(commitMenu(commit, refLabels(commit.refs, data.byFull), ctx), host), fail);
    menu = { items: [...own, ...gitActionEntries(extensions, target, fail)], anchor, label: `Commit ${commit.oid.slice(0, 7)} actions` };
    menuOpen = true;
  }

  function rowMenu(i: number, e: MouseEvent) {
    e.preventDefault();
    const commit = data.commits[i]!;
    selected = commit.oid;
    openMenu(commit, { x: e.clientX, y: e.clientY });
  }

  function rowElement(i: number): HTMLElement | null {
    const oid = data.commits[i]?.oid;
    return oid ? (list?.querySelector<HTMLElement>(`[data-oid="${oid}"]`) ?? null) : null;
  }

  function onListKey(e: KeyboardEvent) {
    const page = Math.max(1, Math.floor(viewport / ROW) - 1);
    const i = selectedIndex;
    switch (e.key) {
      case "ArrowDown":
      case "j":
        move(i + 1);
        break;
      case "ArrowUp":
      case "k":
        move(i < 0 ? 0 : i - 1);
        break;
      case "PageDown":
        move(i + page);
        break;
      case "PageUp":
        move(i - page);
        break;
      case "Home":
        move(0);
        break;
      case "End":
        move(n - 1);
        break;
      case " ":
        if (!selectedCommit) return;
        // Space compares with the parent (marks it as the start); again: off.
        base = base ? null : (selectedCommit.parents[0] ?? null);
        break;
      case "Enter": {
        const file = document.querySelector<HTMLElement>('[data-testid="graph-files"] [aria-selected="true"]');
        if (!file) return;
        file.focus();
        break;
      }
      case "Escape":
        if (!base) return;
        base = null;
        break;
      case "ContextMenu":
        if (!selectedCommit) return;
        openMenu(selectedCommit, rowElement(selectedIndex) ?? list!);
        break;
      case "F10":
        if (!e.shiftKey || !selectedCommit) return;
        openMenu(selectedCommit, rowElement(selectedIndex) ?? list!);
        break;
      default:
        return;
    }
    e.preventDefault();
  }

  function onViewKey(e: KeyboardEvent) {
    if (e.key !== "/" || e.defaultPrevented) return;
    const t = e.target as HTMLElement | null;
    if (t?.closest("input, textarea, [contenteditable='true']")) return;
    e.preventDefault();
    search?.focus();
    search?.select();
  }

  function jumpHit(step: 1 | -1) {
    if (hits.length === 0) return;
    let k: number;
    if (hitPos >= 0) k = (hitPos + step + hits.length) % hits.length;
    else if (step > 0) k = Math.max(0, hits.findIndex((h) => h > selectedIndex));
    else {
      let below = -1;
      for (let j = hits.length - 1; j >= 0 && below < 0; j--) if (hits[j]! < selectedIndex) below = j;
      k = below >= 0 ? below : hits.length - 1;
    }
    const i = hits[k]!;
    selected = data.commits[i]!.oid;
    reveal(i, true);
  }

  async function onSearchKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (hits.length > 0) jumpHit(e.shiftKey ? -1 : 1);
      else if (hexQuery) await goTo(query.trim().toLowerCase(), false);
    } else if (e.key === "Escape") {
      e.preventDefault();
      query = "";
      list?.focus({ preventScroll: true });
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      list?.focus({ preventScroll: true });
    }
  }

  function onSearchInput() {
    // Jump to the first match from the selection on, as you type.
    if (hits.length === 0 || hitSet.has(selectedIndex)) return;
    const i = hits.find((h) => h >= selectedIndex) ?? hits[0]!;
    selected = data.commits[i]!.oid;
    reveal(i, true);
  }

  const searchStatus = $derived.by(() => {
    if (!query.trim()) return "";
    if (hits.length === 0) return hexQuery && data.more ? "Enter: look further" : "No match";
    return `${hitPos >= 0 ? hitPos + 1 : "–"} of ${hits.length}${data.more ? "+" : ""}`;
  });
  const countText = $derived(n === 0 ? "" : `${n.toLocaleString("en-US")}${data.more ? "+" : ""} commit${n === 1 && !data.more ? "" : "s"}`);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="graph-view" data-testid="git-graph" data-select={select ?? null} onkeydown={onViewKey}>
  <ExtensionGitHint target={hintTarget} />
  <div class="toolbar">
    <div class="search">
      <TextField
        size="sm"
        icon="search"
        placeholder="Search message, author, id or branch"
        aria-label="Search the history"
        bind:value={query}
        bind:element={search}
        oninput={onSearchInput}
        onkeydown={onSearchKey}
        data-testid="graph-search"
      >
        {#snippet trailing()}
          {#if searchStatus}<span class="hits" data-testid="graph-search-status" aria-live="polite">{searchStatus}</span>{/if}
        {/snippet}
      </TextField>
    </div>
    <IconButton icon="chevron-up" size="sm" label="Previous match" shortcut="Shift+Enter" disabled={hits.length === 0} onclick={() => jumpHit(-1)} />
    <IconButton icon="chevron-down" size="sm" label="Next match" shortcut="Enter" disabled={hits.length === 0} onclick={() => jumpHit(1)} />
    <Button size="sm" variant="ghost" icon="search" data-testid="graph-search-all" onclick={() => host.views.open({ kind: "search", query: query.trim() ? { message: query.trim() } : undefined })}>Search all history</Button>
    <span class="spacer"></span>
    {#if notice}<span class="notice" role="status" data-testid="graph-notice">{notice}</span>{/if}
    <ExtensionActions actions={rangeActions} label="Range actions" />
    <span class="count" data-testid="graph-count">{countText}</span>
    <SegmentedControl
      size="sm"
      label="Which branches"
      value={scope}
      onchange={setScope}
      options={[
        { value: "all", label: "All branches" },
        { value: "current", label: "Current" },
      ]}
      data-testid="graph-scope"
    />
    <IconButton icon="sync" size="sm" label="Reload the history" disabled={data.loading} onclick={() => void reload()} data-testid="graph-reload" />
  </div>

  {#if !data.ready}
    <div class="skeleton" aria-busy="true">
      {#each Array.from({ length: 12 }, (_, i) => i) as i (i)}
        <div class="skeleton-row"><Skeleton circle width="10px" height="10px" /><Skeleton width="{40 + ((i * 37) % 45)}%" height="10px" /></div>
      {/each}
    </div>
  {:else if data.error && n === 0}
    <EmptyState
      icon="history"
      title="No history to show"
      text="This folder is not in a Git repository, or the repository cannot be read. Start one from the Git panel, then its history shows here."
      data-testid="graph-empty"
    >
      <p class="detail">{data.error}</p>
    </EmptyState>
  {:else if n === 0}
    <EmptyState icon="commit" title="No commits yet" text="Commit your first change and it appears here, with every branch and tag that follows." data-testid="graph-empty" />
  {:else}
    <div class="body">
      <div class="list-pane" style:--graph-w="{Math.max(52, lanes * LANE + 6)}px">
        <div class="columns" aria-hidden="true">
          <span>Graph</span><span>Message</span><span class="c-author">Author</span><span class="c-date">Date</span><span class="c-sha">Commit</span>
        </div>
        <div
          class="graph-list"
          role="listbox"
          tabindex="0"
          aria-label="Commits, newest first"
          aria-activedescendant={selected && selectedIndex >= first && selectedIndex < last ? `graph-row-${selected}` : undefined}
          data-testid="graph-list"
          bind:this={list}
          bind:clientHeight={viewport}
          onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
          onkeydown={onListKey}
        >
          <div class="spacer-rows" style:height="{n * ROW}px">
            {#each visible as i (i % pool)}
              {@const commit = data.commits[i]!}
              <GraphRow
                id="graph-row-{commit.oid}"
                position={i + 1}
                total={n}
                {commit}
                row={data.rows[i]!}
                labels={refLabels(commit.refs, data.byFull)}
                top={i * ROW}
                height={ROW}
                lane={LANE}
                {lanes}
                selected={commit.oid === selected}
                marked={commit.oid === base}
                match={hitSet.has(i)}
                head={commit.oid === data.head}
                {now}
                onpick={(e) => pick(i, e)}
                onmenu={(e) => rowMenu(i, e)}
                ondragstate={(d) => (dragging = d)}
              />
            {/each}
          </div>
          {#if data.loading && data.more}<div class="loading-more" style:top="{n * ROW}px">Loading more…</div>{/if}
        </div>
      </div>
      <aside class="details-pane">
        {#if selectedCommit}
          <CommitDetails commit={selectedCommit} base={baseCommit} byFull={data.byFull} {ctx} {host} {now} onclearbase={() => (base = null)} />
        {:else}
          <EmptyState size="sm" icon="commit" title="Pick a commit" text="Its message, author and changed files show here." />
        {/if}
      </aside>
    </div>
  {/if}
</div>

{#if menu}
  <Menu items={menu.items} anchor={menu.anchor} label={menu.label} bind:open={menuOpen} onclose={() => (menu = null)} />
{/if}

<style>
  .graph-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    container: graph-view / inline-size;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    flex: none;
    padding: var(--rt-space-4) var(--rt-space-6);
    border-bottom: 1px solid var(--rt-border);
    min-width: 0;
  }
  .search {
    flex: 0 1 360px;
    min-width: 160px;
  }
  .hits {
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .count,
  .notice {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .notice {
    color: var(--rt-ok-fg);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(320px, 38%);
  }
  .list-pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    container: graph-list / inline-size;
    --author-w: 160px;
    --date-w: 104px;
    --sha-w: 64px;
  }
  .columns {
    display: grid;
    grid-template-columns: var(--graph-w) minmax(0, 1fr) var(--author-w) var(--date-w) var(--sha-w);
    gap: var(--rt-space-4);
    flex: none;
    padding-right: var(--rt-space-5);
    height: var(--rt-control-sm);
    align-items: center;
    border-bottom: 1px solid var(--rt-border);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    font-weight: var(--rt-weight-semibold);
    background: var(--rt-surface-sunken);
  }
  .columns span:first-child {
    padding-left: var(--rt-space-4);
    overflow: hidden;
  }
  .c-date {
    text-align: right;
  }
  .graph-list {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    outline: none;
  }
  .spacer-rows {
    position: relative;
  }
  .loading-more {
    position: absolute;
    left: 0;
    right: 0;
    padding: var(--rt-space-4);
    text-align: center;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .details-pane {
    min-width: 0;
    min-height: 0;
    overflow: auto;
    border-left: 1px solid var(--rt-border);
    background: var(--rt-surface-1);
  }
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
    padding: var(--rt-space-6);
  }
  .skeleton-row {
    display: flex;
    align-items: center;
    gap: var(--rt-space-5);
  }
  .detail {
    margin: 0;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    font-family: var(--rt-font-mono);
  }
  @container graph-list (max-width: 680px) {
    .columns,
    .graph-list {
      --author-w: 18px;
      --date-w: 96px;
    }
    .c-author {
      visibility: hidden;
    }
  }
  @container graph-list (max-width: 520px) {
    .columns,
    .graph-list {
      --date-w: 84px;
      --sha-w: 0px;
    }
    .c-sha {
      visibility: hidden;
    }
  }
  @container graph-view (max-width: 980px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);
    }
    .details-pane {
      border-left: 0;
      border-top: 1px solid var(--rt-border);
    }
    .count {
      display: none;
    }
  }
</style>

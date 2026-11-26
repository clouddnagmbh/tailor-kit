<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- "Recent commits" of the Git panel: a mini lane graph (lane colours `--rt-lane-*`), the
     subject, the author, a relative time and the branch and tag labels. A click shows what the
     commit changed (by field); its menu (the row's "more" button, right click or the context-menu
     key) holds compare, copy here, undo, branch from here and tag. "Open graph" opens the whole
     graph. The list takes the panel's free height and shows as many commits as fit (at least
     `MIN_ROWS`), so a larger window shows more. -->
<script lang="ts">
  import type { GitGraphCommit, GitRef } from "../types";
  import { Avatar, Button, Chip, IconButton, Menu, Section, type MenuEntry } from "@tailor-kit/ui";
  import { GLOSSARY } from "../glossary";
  import { miniLanes, refPills, shortAgo } from "./model";

  let {
    commits,
    refs,
    head,
    onshow,
    actions,
    ongraph,
  }: {
    commits: GitGraphCommit[];
    refs: GitRef[];
    /** The commit `HEAD` points to. */
    head: string | null;
    onshow: (oid: string) => void;
    /** The menu of one commit. */
    actions: (c: GitGraphCommit) => MenuEntry[];
    ongraph: () => void;
  } = $props();

  const LANE = 12;
  const ROW = 40;
  const MIN_ROWS = 3;
  /** Height of the list's box and where the rows start in it (below the section title). */
  let boxHeight = $state(0);
  let list = $state<HTMLElement | null>(null);
  const fit = $derived(
    boxHeight > 0 && list ? Math.max(MIN_ROWS, Math.floor((boxHeight - list.offsetTop) / ROW)) : commits.length,
  );
  const shown = $derived(commits.slice(0, fit));
  const rows = $derived(miniLanes(shown));
  const width = $derived(Math.max(1, ...rows.flatMap((r) => [r.lane, ...r.above, ...r.below, ...r.joins])) * LANE + LANE);
  const x = (lane: number) => lane * LANE + LANE / 2;
  const color = (lane: number) => `var(--rt-lane-${(lane % 8) + 1})`;
  const MAX_PILLS = 2;
  const PILL_TERM = { local: "branch", remote: "remote-branch", tag: "tag", head: "head" } as const;
  const PILL_ICON = { local: "branch", remote: "service", tag: "tag", head: "commit" } as const;

  /** The open commit menu: which commit and where. */
  let menu = $state<{ commit: GitGraphCommit; anchor: HTMLElement | { x: number; y: number } } | null>(null);
  let menuOpen = $state(false);

  function openMenu(commit: GitGraphCommit, anchor: HTMLElement | { x: number; y: number }) {
    menu = { commit, anchor };
    menuOpen = true;
  }
</script>

<div class="recent" bind:clientHeight={boxHeight}>
  <Section title="Commits">
    {#snippet actions()}
      <Button variant="ghost" size="sm" icon="graph" data-testid="git-open-graph" title="All branches and commits as a graph" onclick={ongraph}>Open graph</Button>
    {/snippet}
    <ol class="log" data-testid="git-log" bind:this={list}>
      {#each shown as c, i (c.oid)}
        {@const row = rows[i]}
        {@const pills = refPills(c.oid, refs)}
        <li data-testid="git-log-row" data-oid={c.oid} class:open={menuOpen && menu?.commit.oid === c.oid}>
          <svg class="lanes" {width} height={ROW} aria-hidden="true">
            {#each row.above as l (l)}
              <line x1={x(l)} y1="0" x2={x(l)} y2={ROW / 2} stroke={color(l)} />
            {/each}
            {#each row.below as l (l)}
              {#if l === row.lane || !row.joins.includes(l)}
                <line x1={x(l)} y1={ROW / 2} x2={x(l)} y2={ROW} stroke={color(l)} />
              {/if}
            {/each}
            {#each row.joins as l (l)}
              {#if row.below.includes(l)}
                <!-- A merge: the second parent's lane leaves the commit. -->
                <path d="M {x(row.lane)} {ROW / 2} C {x(l)} {ROW / 2}, {x(l)} {ROW / 2}, {x(l)} {ROW}" stroke={color(l)} fill="none" />
              {:else}
                <!-- A branch that started here joins from above. -->
                <path d="M {x(l)} 0 C {x(l)} {ROW / 2}, {x(l)} {ROW / 2}, {x(row.lane)} {ROW / 2}" stroke={color(l)} fill="none" />
              {/if}
            {/each}
            <circle
              cx={x(row.lane)}
              cy={ROW / 2}
              r={c.oid === head ? 4.5 : 3.5}
              fill={c.oid === head ? color(row.lane) : "var(--rt-surface-1)"}
              stroke={color(row.lane)}
            />
          </svg>
          <button
            class="row"
            data-testid="git-commit-changes"
            title="{c.summary}&#10;{c.oid.slice(0, 10)} by {c.author}"
            onclick={() => onshow(c.oid)}
            oncontextmenu={(e) => {
              e.preventDefault();
              openMenu(c, { x: e.clientX, y: e.clientY });
            }}
            onkeydown={(e) => {
              if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
                e.preventDefault();
                openMenu(c, e.currentTarget);
              }
            }}
          >
            <span class="subject" data-testid="git-commit-subject">{c.summary}</span>
            <span class="meta">
              <Avatar name={c.author} size="sm" title={c.author} />
              <span class="time">{shortAgo(c.time)}</span>
              {#each pills.slice(0, MAX_PILLS) as p (p.name)}
                <span class="pill" data-testid="git-ref-pill" title="{p.name}. {GLOSSARY[PILL_TERM[p.kind]].label}: {GLOSSARY[PILL_TERM[p.kind]].text}">
                  <Chip size="sm" mono label={p.name} icon={PILL_ICON[p.kind]} tone={p.kind === "local" && p.current ? "info" : undefined} selected={p.kind === "local" && p.current} />
                </span>
              {/each}
              {#if pills.length > MAX_PILLS}
                <span class="pill more-pills" title={pills.slice(MAX_PILLS).map((p) => p.name).join(", ")}><Chip size="sm" label="+{pills.length - MAX_PILLS}" /></span>
              {/if}
            </span>
          </button>
          <span class="more">
            <IconButton
              icon="more"
              size="sm"
              label="More actions"
              data-testid="git-commit-menu"
              aria-haspopup="menu"
              aria-expanded={menuOpen && menu?.commit.oid === c.oid}
              onclick={(e: MouseEvent) => openMenu(c, e.currentTarget as HTMLElement)}
            />
          </span>
        </li>
      {/each}
    </ol>
  </Section>
</div>
{#if menu}
  <Menu
    items={actions(menu.commit)}
    anchor={menu.anchor}
    bind:open={menuOpen}
    placement="bottom-end"
    label="Actions for {menu.commit.summary}"
    onclose={() => (menu = null)}
  />
{/if}

<style>
  /* Grows into the panel's free height; `offsetTop` of the list is measured from here. */
  .recent {
    position: relative;
    flex: 1 1 0;
    min-height: calc(var(--rt-space-8) + 3 * 40px);
    overflow: hidden;
  }
  .log {
    margin: 0 calc(-1 * var(--rt-space-3));
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: stretch;
    gap: var(--rt-space-2);
    min-width: 0;
    padding: 0 var(--rt-space-3);
    border-radius: var(--rt-radius-md);
  }
  li:hover,
  li.open {
    background: var(--rt-bg-hover);
  }
  .lanes {
    flex: none;
    stroke-width: 1.5;
  }
  /* A list row: the row is the control (README: rows use the row tokens). */
  .row {
    display: flex;
    flex: 1;
    flex-direction: column;
    justify-content: center;
    gap: var(--rt-space-1);
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--rt-fg);
    font: var(--rt-font-body);
    text-align: left;
    cursor: pointer;
  }
  .row:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: calc(-1 * var(--rt-focus-width));
    border-radius: var(--rt-radius-sm);
  }
  .subject {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    min-width: 0;
    overflow: hidden;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .meta :global(.rt-avatar) {
    flex: none;
  }
  .time {
    flex: none;
  }
  .pill {
    display: inline-flex;
    flex: 0 1 auto;
    min-width: 3em;
    max-width: 10em;
    overflow: hidden;
  }
  .pill :global(.rt-chip),
  .pill :global(.hit) {
    min-width: 0;
    max-width: 100%;
  }
  .pill :global(.label) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more-pills {
    flex: none;
    min-width: 0;
  }
  .more {
    display: inline-flex;
    align-self: center;
    opacity: 0;
  }
  li:hover .more,
  li.open .more,
  .more:focus-within {
    opacity: 1;
  }
</style>

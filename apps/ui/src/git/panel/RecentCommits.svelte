<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- "Recent commits" of the Git panel: a mini lane graph (lane colours `--rt-lane-*`), the
     subject, the author's initials, a relative time and the branch and tag labels. A click shows
     what the commit changed (by field); its menu (button or context-menu key / right click) holds
     compare, copy here, undo, branch from here and tag. "Open graph" opens the full graph. -->
<script lang="ts">
  import type { GitGraphCommit, GitRef } from "../../commands/types";
  import { GLOSSARY } from "../glossary";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import { authorHue, initials, miniLanes, refPills, shortAgo } from "./model";

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
    actions: (c: GitGraphCommit) => MenuItem[];
    ongraph: () => void;
  } = $props();

  const LANE = 12;
  const ROW = 40;
  const rows = $derived(miniLanes(commits));
  const width = $derived(Math.max(1, ...rows.flatMap((r) => [r.lane, ...r.above, ...r.below, ...r.joins])) * LANE + LANE);
  const x = (lane: number) => lane * LANE + LANE / 2;
  const color = (lane: number) => `var(--rt-lane-${(lane % 8) + 1})`;
  const MAX_PILLS = 2;
  const PILL_TERM = { local: "branch", remote: "remote-branch", tag: "tag", head: "head" } as const;
</script>

<section class="recent" aria-label="Recent commits">
  <header class="section-head">
    <h3>Commits</h3>
    <button class="link" data-testid="git-open-graph" title="All branches and commits as a graph" onclick={ongraph}>Open graph</button>
  </header>
  <ol data-testid="git-log">
    {#each commits as c, i (c.oid)}
      {@const row = rows[i]}
      {@const pills = refPills(c.oid, refs)}
      <li data-testid="git-log-row" data-oid={c.oid}>
        <svg class="lanes" width={width} height={ROW} aria-hidden="true">
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
            fill={c.oid === head ? color(row.lane) : "var(--rt-bg)"}
            stroke={color(row.lane)}
          />
        </svg>
        <button
          class="commit"
          data-testid="git-commit-changes"
          title="{c.summary}&#10;{c.oid.slice(0, 10)} by {c.author}"
          onclick={() => onshow(c.oid)}
          oncontextmenu={(e) => {
            e.preventDefault();
            (e.currentTarget.parentElement?.querySelector('[data-testid="git-commit-menu"]') as HTMLElement | null)?.click();
          }}
        >
          <span class="subject" data-testid="git-commit-subject">{c.summary}</span>
          <span class="meta">
            <span class="avatar" style:--hue="var(--rt-lane-{authorHue(c.author)})" title={c.author}>{initials(c.author)}</span>
            <span class="time">{shortAgo(c.time)}</span>
            {#each pills.slice(0, MAX_PILLS) as p (p.name)}
              <span class="pill {p.kind}" class:current={p.current} data-testid="git-ref-pill" title="{p.name}. {GLOSSARY[PILL_TERM[p.kind]].label}: {GLOSSARY[PILL_TERM[p.kind]].text}">{p.name}</span>
            {/each}
            {#if pills.length > MAX_PILLS}
              <span class="pill more-pills" title={pills.slice(MAX_PILLS).map((p) => p.name).join(", ")}>+{pills.length - MAX_PILLS}</span>
            {/if}
          </span>
        </button>
        <Menu items={actions(c)} label="Actions for {c.summary}" testid="git-commit-menu" align="end" triggerClass="more" title="More actions">
          {#snippet trigger()}<span aria-hidden="true">⋯</span>{/snippet}
        </Menu>
      </li>
    {/each}
  </ol>
</section>

<style>
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: stretch;
    gap: 4px;
    min-width: 0;
    border-radius: var(--rt-radius);
  }
  li:hover {
    background: var(--rt-bg-hover);
  }
  .lanes {
    flex: none;
    stroke-width: 1.5;
  }
  .commit {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    padding: 0;
    border: none;
    background: none;
    color: var(--rt-fg);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .subject {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .avatar {
    flex: none;
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--hue);
    color: var(--rt-bg);
    font-size: 8px;
    font-weight: 700;
  }
  .time {
    flex: none;
  }
  .pill {
    flex: 0 1 auto;
    min-width: 2.5em;
    max-width: 10em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 5px;
    border: 1px solid var(--rt-border);
    border-radius: 8px;
    font-size: 11px;
    line-height: 15px;
  }
  .pill.local.current {
    border-color: var(--rt-accent);
    color: var(--rt-accent);
  }
  .more-pills {
    flex: none;
    min-width: 0;
  }
  .pill.tag {
    border-style: dashed;
  }
  li :global(.more) {
    align-self: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: none;
    color: var(--rt-fg-muted);
    opacity: 0;
    cursor: pointer;
  }
  li:hover :global(.more),
  li :global(.more:focus-visible),
  li :global(.more[aria-expanded="true"]) {
    opacity: 1;
  }
</style>

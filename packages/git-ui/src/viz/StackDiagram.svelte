<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- A stack of branches as a vertical chain (wave 8, strand E; V11): the trunk at the bottom, each
     branch on top of the one below it, the pull request state on the card and an arrow where a
     branch has to be moved onto its parent again ("needs restack"). Cards are links to the
     branch: click selects. -->
<script lang="ts">
  export type StackPullState = "open" | "draft" | "merged" | "closed" | "none";

  export interface StackNode {
    id: string;
    label: string;
    /** Commits of its own on top of the one below. */
    commits: number;
    pull?: { number: number; state: StackPullState; url?: string } | null;
    /** It must be moved onto its parent again. */
    needsRestack?: boolean;
    /** Is on the server. */
    published?: boolean;
    current?: boolean;
    /** Another line under the name ("2 to push"). */
    note?: string;
  }

  let {
    trunk,
    nodes,
    selected = null,
    onselect,
  }: {
    trunk: string;
    /** Bottom first. */
    nodes: readonly StackNode[];
    selected?: string | null;
    onselect?: (id: string) => void;
  } = $props();

  const top = $derived([...nodes].reverse());
  const plural = (n: number) => (n === 1 ? "1 commit" : `${n} commits`);
  const prWord: Record<StackPullState, string> = { open: "open", draft: "draft", merged: "merged", closed: "closed", none: "" };
</script>

<ol class="stack" data-testid="stack-diagram" data-nodes={nodes.length} aria-label="Stack of {nodes.length} branches on {trunk}, top first">
  {#each top as n, i (n.id)}
    <li class="node" class:current={n.current} data-testid="stack-node" data-id={n.id} data-restack={n.needsRestack ? "true" : "false"}>
      <span class="rail" aria-hidden="true">
        <span class="dot" class:ring={!n.published}></span>
        <span class="line" class:restack={n.needsRestack}>
          {#if n.needsRestack}<span class="arrow">↻</span>{/if}
        </span>
      </span>
      <button type="button" class="card" class:selected={selected === n.id} aria-pressed={selected === n.id} onclick={() => onselect?.(n.id)} aria-label="{n.label}, {plural(n.commits)} on {top[i + 1]?.label ?? trunk}{n.needsRestack ? ', needs a restack' : ''}{n.pull ? `, pull request ${n.pull.number} ${prWord[n.pull.state]}` : ''}">
        <span class="top-row">
          <span class="name">{n.label}</span>
          {#if n.pull}<span class="pr {n.pull.state}" data-testid="stack-node-pr">#{n.pull.number} {prWord[n.pull.state]}</span>{:else if n.published === false}<span class="pr none">not pushed</span>{/if}
        </span>
        <span class="sub">{plural(n.commits)} on {top[i + 1]?.label ?? trunk}{n.note ? ` · ${n.note}` : ""}</span>
        {#if n.needsRestack}<span class="restack-note" data-testid="stack-node-restack">needs a restack: {top[i + 1]?.label ?? trunk} has moved on</span>{/if}
      </button>
    </li>
  {/each}
  <li class="node trunk" data-testid="stack-trunk">
    <span class="rail" aria-hidden="true"><span class="dot solid"></span></span>
    <span class="base"><span class="name">{trunk}</span><span class="sub">trunk</span></span>
  </li>
</ol>

<style>
  .stack {
    --rail: 30px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .node {
    display: grid;
    grid-template-columns: var(--rail) minmax(0, 1fr);
    align-items: stretch;
    gap: var(--rt-space-4);
  }
  .rail {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .dot {
    flex: none;
    z-index: 1;
    width: 16px;
    height: 16px;
    margin-top: 18px;
    border-radius: 50%;
    background: var(--rt-accent);
    border: 3px solid var(--rt-surface-1);
    box-shadow: 0 0 0 2px var(--rt-accent);
  }
  .dot.ring {
    background: var(--rt-surface-1);
    border: 3px solid var(--rt-accent);
    box-shadow: none;
  }
  .dot.solid {
    background: var(--rt-fg-muted);
    box-shadow: 0 0 0 2px var(--rt-fg-muted);
  }
  .line {
    position: absolute;
    top: 34px;
    bottom: -18px;
    left: calc(50% - 1.5px);
    width: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--rt-accent) 55%, var(--rt-surface-1));
  }
  .line.restack {
    background: repeating-linear-gradient(to bottom, var(--rt-warn) 0 6px, transparent 6px 10px);
  }
  .arrow {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--rt-warn-bg);
    border: 1px solid var(--rt-warn-border);
    color: var(--rt-warn-fg);
    font-size: 13px;
    line-height: 1;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: var(--rt-space-3) 0;
    padding: var(--rt-space-4) var(--rt-space-5);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-2);
    box-shadow: var(--rt-elev-1);
    text-align: left;
    color: var(--rt-fg);
    cursor: pointer;
    transition: border-color var(--rt-dur-fast) var(--rt-ease);
  }
  .card:hover,
  .card.selected {
    border-color: var(--rt-accent);
  }
  .card:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .current .card {
    background: var(--rt-accent-subtle);
  }
  .top-row {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    min-width: 0;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--rt-font-title);
  }
  .sub {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .pr {
    flex: none;
    padding: 1px 8px;
    border-radius: var(--rt-radius-pill);
    font: var(--rt-font-caption);
    font-variant-numeric: tabular-nums;
    background: var(--rt-info-bg);
    color: var(--rt-info-fg);
  }
  .pr.merged {
    background: var(--rt-ok-bg);
    color: var(--rt-ok-fg);
  }
  .pr.closed {
    background: var(--rt-danger-bg);
    color: var(--rt-danger-fg);
  }
  .pr.draft,
  .pr.none {
    background: var(--rt-surface-sunken);
    color: var(--rt-fg-muted);
  }
  .restack-note {
    color: var(--rt-warn-fg);
    font: var(--rt-font-small);
  }
  .trunk .base {
    display: flex;
    align-items: baseline;
    gap: var(--rt-space-4);
    padding: var(--rt-space-4) 0;
  }
  .trunk .dot {
    margin-top: 12px;
  }
</style>

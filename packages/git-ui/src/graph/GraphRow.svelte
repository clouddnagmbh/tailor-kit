<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- Wave 14, strand F: one row of the commit graph: its part of the lanes (`layout.ts`, drawn in
     its own height so the list can be virtualised), the ref labels, the message, the author,
     the date and the short id. An option of the graph's listbox; the list handles keys. -->
<script lang="ts">
  import type { GraphDragSource } from "../extensions";
  import type { GitGraphCommit } from "../types";
  import { getGitHost } from "../host";
  import { Avatar, tooltip } from "@tailor-kit/ui";
  import { laneColor, rowPaths, type LaneRow } from "./layout";
  import type { RefLabel as Label } from "./model";
  import { absoluteTime, relativeTime, shortId } from "./model";
  import RefLabel from "./RefLabel.svelte";

  let {
    id,
    position,
    total,
    commit,
    row,
    labels,
    top,
    height,
    lane,
    lanes,
    selected,
    marked,
    match,
    head,
    now,
    onpick,
    onmenu,
    ondragstate,
    drop = null,
    dropLabel = "",
  }: {
    id: string;
    position: number;
    total: number;
    commit: GitGraphCommit;
    row: LaneRow;
    labels: Label[];
    /** Offset in the virtual list, px. */
    top: number;
    height: number;
    /** Width of one lane, px. */
    lane: number;
    /** Lanes the graph column has room for. */
    lanes: number;
    selected: boolean;
    marked: boolean;
    match: boolean;
    head: boolean;
    now: number;
    onpick: (e: MouseEvent) => void;
    onmenu: (e: MouseEvent) => void;
    /** The row (or one of its branch labels, `source`) is being dragged (true) or was dropped
     *  or cancelled (false). */
    ondragstate?: (dragging: boolean, source?: GraphDragSource) => void;
    /** A drop an extension accepts would land in this half of the row. */
    drop?: "above" | "below" | null;
    /** Wave 8 (additive): names the action the drop would offer, at the end of the row. */
    dropLabel?: string;
  } = $props();
  const host = getGitHost();

  function dragstart(e: DragEvent) {
    // A branch or tag label of the row drags itself (since tailor-kit 0.3): the graph's drop
    // targets see which; anything else gets its name to paste.
    const label = (e.target as HTMLElement | null)?.closest?.<HTMLElement>("[data-ref]");
    const ref = label?.dataset.ref;
    if (ref) {
      e.dataTransfer?.setData("text/plain", label?.dataset.name ?? ref);
      if (e.dataTransfer) e.dataTransfer.effectAllowed = "copyMove";
      ondragstate?.(true, { kind: "ref", ref, commit: commit.oid });
      return;
    }
    // The commit id travels with the drag: an editor that reorders commits reads it, anything
    // else gets a plain id to paste.
    e.dataTransfer?.setData("text/plain", commit.oid);
    e.dataTransfer?.setData(host.brand.commitMime, commit.oid);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "copyMove";
    ondragstate?.(true, { kind: "commit", commit: commit.oid });
  }

  const x = (l: number) => lane / 2 + l * lane;
  const paths = $derived(rowPaths(row, x, height));
  const width = $derived(lanes * lane);
  const shown = $derived(labels.slice(0, 3));
  const hidden = $derived(labels.length - shown.length);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  {id}
  class="row"
  class:selected
  class:marked
  class:match
  class:head
  class:merge={row.merge}
  class:drop-above={drop === "above"}
  class:drop-below={drop === "below"}
  data-drop={drop}
  role="option"
  tabindex="-1"
  aria-selected={selected}
  aria-posinset={position}
  aria-setsize={total}
  aria-label="{commit.summary}, {commit.author}, {relativeTime(commit.time, now)}"
  data-testid="graph-row"
  data-oid={commit.oid}
  draggable="true"
  ondragstart={dragstart}
  ondragend={() => ondragstate?.(false)}
  style:transform="translateY({top}px)"
  style:height="{height}px"
  onclick={onpick}
  oncontextmenu={onmenu}
>
  <svg class="lanes" width={width} height={height} viewBox="0 0 {width} {height}" aria-hidden="true">
    {#each paths as p, i (i)}
      <path d={p.d} stroke={laneColor(p.color)} />
    {/each}
    {#if head}
      <circle class="halo" cx={x(row.lane)} cy={height / 2} r={7} stroke={laneColor(row.color)} />
    {/if}
    <circle
      class="dot"
      class:hollow={row.merge}
      cx={x(row.lane)}
      cy={height / 2}
      r={row.merge ? 3.5 : 4.5}
      fill={row.merge ? "var(--rt-surface-1)" : laneColor(row.color)}
      stroke={laneColor(row.color)}
    />
  </svg>
  <span class="message">
    {#each shown as l (l.ref.full)}
      <RefLabel label={l} draggable={l.ref.kind !== "head"} />
    {/each}
    {#if hidden > 0}<span class="more" use:tooltip={labels.slice(3).map((l) => l.ref.name).join(", ")}>+{hidden}</span>{/if}
    <span class="summary" data-testid="graph-summary">{commit.summary}</span>
  </span>
  <span class="author" use:tooltip={`${commit.author} <${commit.email}>`}>
    <Avatar name={commit.author} size="sm" title="" />
    <span class="name">{commit.author}</span>
  </span>
  <span class="date" data-testid="graph-date" use:tooltip={absoluteTime(commit.time)}>{relativeTime(commit.time, now)}</span>
  <span class="sha">{shortId(commit.oid)}</span>
  {#if drop && dropLabel}<span class="drop-label" aria-hidden="true" data-testid="graph-drop-label">{dropLabel}</span>{/if}
</div>

<style>
  .row {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: grid;
    grid-template-columns: var(--graph-w) minmax(0, 1fr) var(--author-w) var(--date-w) var(--sha-w);
    align-items: center;
    gap: var(--rt-space-4);
    padding-right: var(--rt-space-5);
    box-sizing: border-box;
    color: var(--rt-fg);
    font: var(--rt-font-body);
    cursor: default;
    user-select: none;
    contain: layout style;
  }
  .row:hover {
    background: var(--rt-bg-hover);
  }
  .row.match {
    background: color-mix(in srgb, var(--rt-warn-bg) 70%, transparent);
  }
  .row.marked {
    box-shadow: inset 3px 0 0 var(--rt-info-fg);
    background: var(--rt-info-bg);
  }
  .row.selected {
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .drop-label {
    position: absolute;
    right: var(--rt-space-5);
    top: 50%;
    transform: translateY(-50%);
    max-width: 60%;
    padding: 1px var(--rt-space-4);
    border-radius: var(--rt-radius-md);
    background: var(--rt-accent-solid);
    color: var(--rt-accent-solid-fg);
    font: var(--rt-font-caption);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
  }
  .row.drop-above {
    box-shadow: inset 0 2px 0 var(--rt-accent);
  }
  .row.drop-below {
    box-shadow: inset 0 -2px 0 var(--rt-accent);
  }
  :global(.graph-list:focus-visible) .row.selected {
    box-shadow: inset 0 0 0 var(--rt-focus-width) var(--rt-focus);
  }
  .lanes {
    display: block;
    overflow: hidden;
    padding-left: var(--rt-space-3);
  }
  path {
    fill: none;
    stroke-width: 2;
    stroke-linecap: round;
  }
  .dot {
    stroke-width: 2;
  }
  .halo {
    fill: none;
    stroke-width: 1.5;
    opacity: 0.55;
  }
  .message {
    display: flex;
    align-items: center;
    gap: var(--rt-space-2);
    min-width: 0;
    overflow: hidden;
  }
  .summary {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .head .summary {
    font-weight: var(--rt-weight-semibold);
  }
  .merge .summary {
    color: var(--rt-fg-muted);
  }
  .selected .summary {
    color: inherit;
  }
  .more {
    flex: none;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .author {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    min-width: 0;
  }
  .name,
  .date,
  .sha {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .selected .name,
  .selected .date,
  .selected .sha {
    color: inherit;
  }
  .date {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .sha {
    font-family: var(--rt-font-mono);
  }
  @container graph-list (max-width: 680px) {
    .name {
      display: none;
    }
  }
  @container graph-list (max-width: 520px) {
    .sha {
      display: none;
    }
  }
</style>

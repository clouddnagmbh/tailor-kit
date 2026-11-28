<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The conflict radar as a picture (wave 8, strand E; V5): files as rows, the other sides of a
     merge (upstream, default branch, open pull requests) as columns, one cell per pair. A cell is
     never only a colour: it carries a sign. A real table, so it is its own text form. -->
<script lang="ts">
  export type RadarCellState = "clean" | "conflict" | "local" | "none";

  export interface RadarColumn {
    id: string;
    label: string;
    /** "Upstream", "Default branch", "Pull request". */
    kind: string;
  }
  export interface RadarRow {
    path: string;
    /** Column id → state; a missing column is `none` (the other side does not change the file). */
    cells: Readonly<Record<string, RadarCellState>>;
    /** Lines that collide, per column, for the tooltip. */
    notes?: Readonly<Record<string, string>>;
  }

  let {
    columns,
    rows,
    onselect,
  }: {
    columns: readonly RadarColumn[];
    rows: readonly RadarRow[];
    onselect?: (path: string, column: string) => void;
  } = $props();

  const SIGN: Record<RadarCellState, string> = { clean: "✓", conflict: "!", local: "~", none: "" };
  const WORD: Record<RadarCellState, string> = { clean: "merges cleanly", conflict: "collides", local: "local changes in the way", none: "untouched" };
  const stateOf = (r: RadarRow, c: string): RadarCellState => r.cells[c] ?? "none";
</script>

<div class="wrap">
  <table class="matrix" data-testid="radar-matrix" data-rows={rows.length} data-columns={columns.length}>
    <caption class="sr-only">Files that a merge would touch, by the other side of the merge</caption>
    <thead>
      <tr>
        <th scope="col" class="file-head">File</th>
        {#each columns as c (c.id)}
          <th scope="col" class="col-head" data-testid="radar-matrix-column" data-column={c.id}>
            <span class="kind">{c.kind}</span>
            <span class="name">{c.label}</span>
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as r (r.path)}
        <tr data-testid="radar-matrix-row" data-path={r.path}>
          <th scope="row" class="path" title={r.path}>{r.path.replace(/^\//, "")}</th>
          {#each columns as c (c.id)}
            {@const s = stateOf(r, c.id)}
            <td class="cell {s}" data-testid="radar-matrix-cell" data-state={s} data-column={c.id}>
              {#if s === "none"}
                <span class="blank" aria-label={WORD.none}>·</span>
              {:else}
                <button type="button" class="chip" title="{WORD[s]}{r.notes?.[c.id] ? `: ${r.notes[c.id]}` : ''}" aria-label="{r.path.replace(/^\//, '')} with {c.label}: {WORD[s]}{r.notes?.[c.id] ? `, ${r.notes[c.id]}` : ''}" onclick={() => onselect?.(r.path, c.id)}>
                  <span class="sign" aria-hidden="true">{SIGN[s]}</span>
                </button>
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
  <ul class="legend" aria-hidden="true">
    <li><span class="key conflict">!</span> collides</li>
    <li><span class="key local">~</span> local changes in the way</li>
    <li><span class="key clean">✓</span> merges cleanly</li>
    <li><span class="key none">·</span> untouched</li>
  </ul>
</div>

<style>
  .wrap {
    overflow-x: auto;
  }
  .matrix {
    border-collapse: separate;
    border-spacing: 0;
    min-width: 100%;
    font: var(--rt-font-small);
  }
  th,
  td {
    padding: 0;
  }
  .file-head,
  .col-head {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: var(--rt-space-3) var(--rt-space-4);
    background: var(--rt-surface-1);
    border-bottom: 1px solid var(--rt-border);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    text-align: left;
  }
  .col-head {
    min-width: 108px;
    text-align: center;
  }
  .col-head .kind {
    display: block;
    text-transform: uppercase;
    letter-spacing: var(--rt-tracking-caps);
    font-size: 10px;
  }
  .col-head .name {
    display: block;
    color: var(--rt-fg);
    font: var(--rt-font-small);
    max-width: 150px;
    margin: 0 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path {
    max-width: 360px;
    padding: 0 var(--rt-space-5) 0 var(--rt-space-4);
    font: var(--rt-font-small);
    font-family: var(--rt-font-mono);
    font-weight: var(--rt-weight-regular);
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-bottom: 1px solid var(--rt-border);
  }
  td {
    height: 36px;
    text-align: center;
    border-bottom: 1px solid var(--rt-border);
  }
  tr:hover > * {
    background: var(--rt-bg-hover);
  }
  .blank {
    color: var(--rt-fg-subtle);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 52px;
    height: 24px;
    border: 1px solid transparent;
    border-radius: var(--rt-radius-pill);
    font: var(--rt-font-small);
    font-weight: var(--rt-weight-bold);
    cursor: pointer;
  }
  .cell.conflict .chip {
    background: var(--rt-danger-bg);
    color: var(--rt-danger-fg);
    border-color: var(--rt-danger-border);
  }
  .cell.local .chip {
    background: var(--rt-warn-bg);
    color: var(--rt-warn-fg);
    border-color: var(--rt-warn-border);
  }
  .cell.clean .chip {
    background: var(--rt-ok-bg);
    color: var(--rt-ok-fg);
    border-color: var(--rt-ok-border);
  }
  .chip:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-5);
    margin: var(--rt-space-4) 0 0;
    padding: 0;
    list-style: none;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-2);
  }
  .key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 16px;
    border-radius: var(--rt-radius-pill);
    font-weight: var(--rt-weight-bold);
  }
  .key.conflict {
    background: var(--rt-danger-bg);
    color: var(--rt-danger-fg);
  }
  .key.local {
    background: var(--rt-warn-bg);
    color: var(--rt-warn-fg);
  }
  .key.clean {
    background: var(--rt-ok-bg);
    color: var(--rt-ok-fg);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>

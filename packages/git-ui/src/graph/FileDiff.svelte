<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 14, strand F: the line diff of any file of the repository between two revisions, from
     `GitApi.fileDiff` (renames against the old path, binary files as metadata only). Looks like
     the wave-12 line diff (`../../diff/LineDiff.svelte`, which reads collection files itself);
     line numbers on both sides. -->
<script lang="ts">
  import type { GitDiff } from "../types";
  import { hunkHeader } from "../format";
  import { EmptyState, Skeleton } from "@tailor-kit/ui";

  let { diff, error = null }: { diff: GitDiff | null; error?: string | null } = $props();

  const SIGN = { context: " ", added: "+", removed: "-" } as const;

  /** Old and new line numbers of each line of a hunk. */
  function numbered(h: GitDiff["hunks"][number]) {
    let o = h.old_start;
    let n = h.new_start;
    return h.lines.map((l) => {
      const row = { ...l, old: l.kind === "added" ? null : o, new: l.kind === "removed" ? null : n };
      if (l.kind !== "added") o++;
      if (l.kind !== "removed") n++;
      return row;
    });
  }
</script>

<div class="file-diff" data-testid="graph-line-diff">
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !diff}
    <div class="loading"><Skeleton lines={4} label="Loading the diff" /></div>
  {:else if diff.binary}
    <EmptyState icon="file" size="sm" title="Binary file" text="Its content is not text, so there are no lines to compare." />
  {:else if diff.hunks.length === 0}
    <EmptyState icon="check" size="sm" title="No line changes" text="Only the name or the file mode changed." />
  {:else}
    {#each diff.hunks as hunk, i (i)}
      <section class="hunk" data-testid="graph-hunk" aria-label={hunkHeader(hunk)}>
        <div class="hunk-header">{hunkHeader(hunk)}</div>
        {#each numbered(hunk) as line, k (k)}
          <div class="line {line.kind}" data-kind={line.kind}>
            <span class="no">{line.old ?? ""}</span><span class="no">{line.new ?? ""}</span><span class="sign">{SIGN[line.kind]}</span><span class="text">{line.text}</span>
          </div>
        {/each}
      </section>
    {/each}
  {/if}
</div>

<style>
  .file-diff {
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-2);
    overflow: hidden;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
    line-height: var(--rt-leading-small);
  }
  .loading {
    padding: var(--rt-space-5);
  }
  .hunk + .hunk {
    border-top: 1px solid var(--rt-border);
  }
  .hunk-header {
    padding: var(--rt-space-2) var(--rt-space-5);
    color: var(--rt-fg-muted);
    background: var(--rt-surface-sunken);
  }
  .line {
    display: grid;
    grid-template-columns: 4ch 4ch 2ch minmax(0, 1fr);
    padding-right: var(--rt-space-5);
  }
  .no {
    padding-right: var(--rt-space-2);
    color: var(--rt-fg-subtle);
    text-align: right;
    user-select: none;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .line.added {
    background: var(--rt-diff-add-bg);
  }
  .line.removed {
    background: var(--rt-diff-del-bg);
  }
  .sign {
    padding-left: var(--rt-space-2);
    color: var(--rt-fg-muted);
    user-select: none;
  }
  .added .sign {
    color: var(--rt-ok-fg);
  }
  .removed .sign {
    color: var(--rt-danger-fg);
  }
  .error {
    margin: 0;
    padding: var(--rt-space-4) var(--rt-space-5);
    color: var(--rt-danger-fg);
    font: var(--rt-font-small);
  }
</style>

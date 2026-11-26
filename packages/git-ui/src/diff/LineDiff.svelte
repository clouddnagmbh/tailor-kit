<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The line diff of one file in a diff scope (wave 12, strand A): saved files → HEAD through
     `git.diff`, commits and revision pairs through `git.show` on both sides. -->
<script lang="ts">
  import type { DiffScope, GitDiff } from "../types";
  import { hunkHeader } from "../format";
  import { getGitHost } from "../host";
  import { Spinner } from "@tailor-kit/ui";
  import { lineDiff } from "./textdiff";

  let { scope, path }: { scope: DiffScope; path: string } = $props();
  const host = getGitHost();

  let diff = $state<GitDiff | null>(null);
  let error = $state<string | null>(null);

  async function load(s: DiffScope, p: string): Promise<GitDiff> {
    const git = host.backend.git;
    if (s.kind === "working") return git.diff(p);
    const [from, to] = s.kind === "commit" ? [`${s.rev}^`, s.rev] : [s.from, s.to];
    // A root commit has no parent: the file did not exist before it.
    const before = await git.show(from, p).catch(() => null);
    return lineDiff(p, before, await git.show(to, p));
  }

  $effect(() => {
    const s = scope;
    const p = path;
    diff = null;
    error = null;
    load(s, p).then(
      (d) => {
        if (s === scope && p === path) diff = d;
      },
      (e: unknown) => {
        if (s === scope && p === path) error = e instanceof Error ? e.message : String(e);
      },
    );
  });

  const SIGN = { context: " ", added: "+", removed: "-" } as const;
</script>

<div class="lines" data-testid="line-diff" data-path={path}>
  <div class="path">{path}</div>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !diff}
    <p class="hint"><Spinner size="sm" label="Loading" /> Loading…</p>
  {:else if diff.binary}
    <p class="hint">Binary file: no line diff.</p>
  {:else if diff.hunks.length === 0}
    <p class="hint">No line changes.</p>
  {:else}
    {#each diff.hunks as hunk, i (i)}
      <section class="hunk" data-testid="line-hunk" aria-label={hunkHeader(hunk)}>
        <div class="hunk-header">{hunkHeader(hunk)}</div>
        {#each hunk.lines as line, k (k)}
          <div class="line {line.kind}" data-kind={line.kind}><span class="sign">{SIGN[line.kind]}</span><span class="text">{line.text}</span></div>
        {/each}
      </section>
    {/each}
  {/if}
</div>

<style>
  .lines {
    margin: var(--rt-space-5) 0;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-2);
    box-shadow: var(--rt-elev-1);
    overflow: hidden;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
    line-height: var(--rt-leading-small);
  }
  .path {
    padding: var(--rt-space-4) var(--rt-space-5);
    font-weight: var(--rt-weight-semibold);
    background: var(--rt-surface-sunken);
    border-bottom: 1px solid var(--rt-border);
    overflow-wrap: anywhere;
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
    padding: 0 var(--rt-space-5);
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
    display: inline-block;
    width: 2ch;
    color: var(--rt-fg-muted);
    user-select: none;
  }
  .added .sign {
    color: var(--rt-ok-fg);
  }
  .removed .sign {
    color: var(--rt-danger-fg);
  }
  .hint,
  .error {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    margin: 0;
    padding: var(--rt-space-4) var(--rt-space-5);
    font: var(--rt-font-small);
  }
  .hint {
    color: var(--rt-fg-muted);
  }
  .error {
    color: var(--rt-danger-fg);
  }
</style>

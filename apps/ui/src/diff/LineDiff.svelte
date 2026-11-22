<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The line diff of one file in a diff scope (wave 12, strand A): saved files → HEAD through
     `git.diff`, commits and revision pairs through `git.show` on both sides. -->
<script lang="ts">
  import type { DiffScope, GitDiff } from "../commands/types";
  import { hunkHeader } from "../git/format";
  import { getApp } from "../state/app.svelte";
  import { lineDiff } from "./textdiff";

  let { scope, path }: { scope: DiffScope; path: string } = $props();
  const app = getApp();

  let diff = $state<GitDiff | null>(null);
  let error = $state<string | null>(null);

  async function load(s: DiffScope, p: string): Promise<GitDiff> {
    const git = app.backend.git;
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
    <p class="hint">Loading…</p>
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
    margin: var(--rt-gap) 0;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
  }
  .path {
    padding: 2px calc(var(--rt-gap) * 2);
    font-weight: 600;
    background: var(--rt-bg-panel);
    overflow-wrap: anywhere;
  }
  .hunk-header {
    padding: 2px calc(var(--rt-gap) * 2);
    color: var(--rt-fg-muted);
  }
  .line {
    padding: 0 calc(var(--rt-gap) * 2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .line.added {
    background: color-mix(in srgb, var(--rt-ok) 16%, transparent);
  }
  .line.removed {
    background: color-mix(in srgb, var(--rt-danger) 16%, transparent);
  }
  .sign {
    display: inline-block;
    width: 1.5ch;
    user-select: none;
  }
  .hint,
  .error {
    margin: 0;
    padding: 2px calc(var(--rt-gap) * 2);
  }
  .hint {
    color: var(--rt-fg-muted);
  }
  .error {
    color: var(--rt-danger);
  }
</style>

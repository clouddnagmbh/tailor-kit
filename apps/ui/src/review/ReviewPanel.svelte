<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Pull and merge requests: list, open, semantic diff (diff/SemanticDiff.svelte), run the
     changed requests, comment, approve, CI status (wave 12, strand E). The Git host comes from
     the collection's `origin`; the token is the user's own. `number`: open this PR at once. -->
<script lang="ts">
  import { onMount } from "svelte";

  import type { PullRequest, ReviewRepo } from "../commands/types";
  import { getApp } from "../state/app.svelte";
  import { relativeTime, requestWord, stateLabel } from "./model";
  import PullView from "./PullView.svelte";
  import ReviewSetup from "./ReviewSetup.svelte";

  let props: { number?: number } = $props();
  const app = getApp();

  let repo = $state<ReviewRepo | null>(null);
  let detected = $state(false);
  let needsToken = $state(false);
  let tokenMessage = $state<string | null>(null);
  let showSetup = $state(false);
  let pulls = $state<PullRequest[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  // svelte-ignore state_referenced_locally: the prop only seeds the selection.
  let current = $state<number | null>(props.number ?? null);

  const word = $derived(requestWord(repo));

  /** A rejected or missing token: ask for one (again). */
  function authProblem(e: unknown): boolean {
    const kind = (e as { kind?: string }).kind;
    if (kind === "no-token" || kind === "auth") {
      needsToken = true;
      tokenMessage = kind === "auth" ? (e as Error).message : null;
      return true;
    }
    return false;
  }

  async function refresh() {
    loading = true;
    error = null;
    try {
      repo = await app.backend.review.repo();
      detected = true;
      if (!repo) return;
      needsToken = !(await app.backend.review.hasToken(repo.host));
      if (needsToken) return;
      tokenMessage = null;
      pulls = await app.backend.review.list();
    } catch (e) {
      if (!authProblem(e)) error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => void refresh());

  const onAuth = (message: string | null) => {
    current = null;
    needsToken = true;
    tokenMessage = message;
  };
</script>

<section class="review" data-testid="review-panel" aria-label="Pull requests">
  {#if current !== null && repo}
    <PullView number={current} {repo} onback={() => (current = null)} onauth={onAuth} />
  {:else if !detected && loading}
    <p class="hint">Looking for the Git host…</p>
  {:else if repo === null || needsToken || showSetup}
    <ReviewSetup
      {repo}
      {needsToken}
      message={tokenMessage}
      onchange={() => {
        showSetup = false;
        void refresh();
      }}
    />
    {#if repo && !needsToken}
      <div class="bar"><button class="button" onclick={() => (showSetup = false)}>Back to the list</button></div>
    {/if}
    {#if error}<p class="error" role="alert" data-testid="review-error">{error}</p>{/if}
  {:else}
    <div class="bar">
      <span class="muted" data-testid="review-repo">{repo.host} · {repo.project}</span>
      <span class="spacer"></span>
      <button class="button" data-testid="review-refresh" disabled={loading} onclick={() => void refresh()}>{loading ? "Loading…" : "Refresh"}</button>
      <button class="button" data-testid="review-settings" onclick={() => (showSetup = true)}>Token and hosts</button>
    </div>
    {#if error}<p class="error" role="alert" data-testid="review-error">{error}</p>{/if}
    {#if pulls.length === 0 && !loading && !error}
      <p class="hint" data-testid="review-empty">No open {word}s.</p>
    {/if}
    <ul class="list" data-testid="review-list">
      {#each pulls as p (p.number)}
        <li>
          <button class="row" data-testid="review-row" data-number={p.number} onclick={() => (current = p.number)}>
            <span class="num">#{p.number}</span>
            <span class="title">{p.title}</span>
            {#if p.draft}<span class="badge" data-testid="review-draft">Draft</span>{:else if p.state !== "open"}<span class="badge">{stateLabel(p)}</span>{/if}
            <span class="muted branches">{p.author} · {p.source} → {p.target}</span>
            <span class="muted">{relativeTime(p.updated)}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .review {
    display: flex;
    flex-direction: column;
    gap: var(--rt-gap);
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--rt-gap);
    padding: var(--rt-gap) 12px 0;
  }
  .spacer {
    flex: 1;
  }
  .hint,
  .muted {
    color: var(--rt-fg-muted);
  }
  .hint,
  .error {
    margin: 0;
    padding: var(--rt-gap) 12px;
  }
  .error {
    color: var(--rt-danger);
  }
  .button {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    cursor: pointer;
  }
  .button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .list {
    list-style: none;
    margin: 0 12px;
    padding: 0;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
  }
  .list li + li {
    border-top: 1px solid var(--rt-border);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 12px;
    width: 100%;
    padding: var(--rt-gap) 12px;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .row:hover {
    background: var(--rt-bg-hover);
  }
  .num {
    font-family: var(--rt-font-mono);
    color: var(--rt-fg-muted);
  }
  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .branches {
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
  }
  .badge {
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    padding: 0 6px;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
</style>

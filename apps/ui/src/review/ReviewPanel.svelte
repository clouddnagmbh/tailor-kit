<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Pull and merge requests: list, open, semantic diff (diff/SemanticDiff.svelte), run the
     changed requests, comment, approve, CI status (wave 12, strand E). The Git host comes from
     the collection's `origin`; the token is the user's own. `number`: open this PR at once. -->
<script lang="ts">
  import { onMount } from "svelte";

  import type { PullRequest, ReviewRepo } from "../commands/types";
  import { getApp } from "../state/app.svelte";
  import { Avatar, Badge, Banner, Button, Card, Chip, EmptyState } from "../ui";
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
      <div class="bar"><Button onclick={() => (showSetup = false)}>Back to the list</Button></div>
    {/if}
    {#if error}<div class="note" data-testid="review-error"><Banner tone="danger">{error}</Banner></div>{/if}
  {:else}
    <header class="bar">
      <div class="titles">
        <h2>Pull requests</h2>
        <span class="muted" data-testid="review-repo">{repo.host} · {repo.project}</span>
      </div>
      <span class="spacer"></span>
      <Button data-testid="review-refresh" icon="sync" loading={loading} disabled={loading} onclick={() => void refresh()}>{loading ? "Loading…" : "Refresh"}</Button>
      <Button variant="ghost" data-testid="review-settings" icon="settings" onclick={() => (showSetup = true)}>Token and hosts</Button>
    </header>
    {#if error}<div class="note" data-testid="review-error"><Banner tone="danger">{error}</Banner></div>{/if}
    {#if pulls.length === 0 && !loading && !error}
      <div data-testid="review-empty"><EmptyState icon="review" title="No open {word}s." /></div>
    {/if}
    <ul class="list" data-testid="review-list">
      {#each pulls as p (p.number)}
        <li data-testid="review-row" data-number={p.number}>
          <Card padding="sm" onclick={() => (current = p.number)}>
            <div class="row">
              <span class="num">#{p.number}</span>
              <span class="title">{p.title}</span>
              {#if p.draft}<span data-testid="review-draft"><Badge tone="neutral" value="Draft" /></span>{:else if p.state !== "open"}<Badge tone={p.state === "merged" ? "accent" : "neutral"} value={stateLabel(p)} />{/if}
              <span class="spacer"></span>
              <span class="meta">
                <Avatar name={p.author} size="sm" />
                <span class="muted">{p.author}</span>
                <Chip size="sm" mono icon="branch" label={p.source} />
                <span class="muted" aria-hidden="true">→</span>
                <Chip size="sm" mono label={p.target} />
                <span class="muted">{relativeTime(p.updated)}</span>
              </span>
            </div>
          </Card>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .review {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-6);
    padding: var(--rt-space-6);
    min-height: 0;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-4);
  }
  .titles {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font: var(--rt-font-heading);
  }
  .spacer {
    flex: 1;
  }
  .hint,
  .muted {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .hint {
    margin: 0;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-2) var(--rt-space-5);
  }
  .num {
    font: var(--rt-font-small);
    font-family: var(--rt-font-mono);
    color: var(--rt-fg-muted);
  }
  .title {
    font: var(--rt-font-title);
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .meta {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-3);
    min-width: 0;
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 12 interface: shows the open Git-for-APIs view (`views.svelte.ts`) in the main area,
     with a header to close it. Each kind is one strand's component; this file only maps kinds
     to components. -->
<script lang="ts">
  import BisectPanel from "../compare/BisectPanel.svelte";
  import ComparePanel from "../compare/ComparePanel.svelte";
  import ContractPanel from "../contract/ContractPanel.svelte";
  import SemanticDiffView from "../diff/SemanticDiffView.svelte";
  import MergeView from "../merge/MergeView.svelte";
  import RescueWizard from "../rescue/RescueWizard.svelte";
  import ReviewPanel from "../review/ReviewPanel.svelte";
  import RequestTimeline from "../timeline/RequestTimeline.svelte";
  import type { GitView, GitViews } from "./views.svelte";

  let { views }: { views: GitViews } = $props();

  const TITLES: Record<GitView["kind"], string> = {
    diff: "Changes",
    merge: "Merge",
    compare: "Compare revisions",
    bisect: "Bisect",
    timeline: "Request history",
    review: "Pull requests",
    contract: "API contract",
    rescue: "Secret rescue",
  };
</script>

{#if views.current}
  {@const view = views.current}
  <section class="git-view" data-testid="git-view" data-kind={view.kind} aria-label={TITLES[view.kind]}>
    <header>
      <h2>{TITLES[view.kind]}</h2>
      <button class="button" data-testid="git-view-close" onclick={() => views.close()}>Close</button>
    </header>
    <div class="body">
      {#if view.kind === "diff"}
        <SemanticDiffView scope={view.scope} />
      {:else if view.kind === "merge"}
        <MergeView onclose={() => views.close()} />
      {:else if view.kind === "compare"}
        <ComparePanel from={view.from} to={view.to} />
      {:else if view.kind === "bisect"}
        <BisectPanel good={view.good} bad={view.bad} />
      {:else if view.kind === "timeline"}
        <RequestTimeline path={view.path} />
      {:else if view.kind === "review"}
        <ReviewPanel number={view.number} />
      {:else if view.kind === "contract"}
        <ContractPanel />
      {:else}
        <RescueWizard />
      {/if}
    </div>
  </section>
{/if}

<style>
  .git-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--rt-gap);
    height: var(--rt-bar-height);
    padding: 0 var(--rt-gap);
    border-bottom: 1px solid var(--rt-border);
  }
  h2 {
    margin: 0;
    font-size: var(--rt-size);
    font-weight: 600;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
</style>

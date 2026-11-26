<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 12 interface: shows the open Git view in the main area. The generic kinds (graph,
     changes, files, history, review list, text merge, extensions) are drawn by
     `@tailor-kit/git-ui`; this wrapper adds RequestTailor's own kinds (semantic diff, merge,
     compare, bisect, request history, contract, secret rescue). -->
<script lang="ts">
  import GitViewHost from "@tailor-kit/git-ui/GitViewHost.svelte";
  import type { GitView, GitViews } from "@tailor-kit/git-ui";
  import BisectPanel from "../compare/BisectPanel.svelte";
  import ComparePanel from "../compare/ComparePanel.svelte";
  import ContractPanel from "../contract/ContractPanel.svelte";
  import SemanticDiffView from "../diff/SemanticDiffView.svelte";
  import MergeView from "../merge/MergeView.svelte";
  import RescueWizard from "../rescue/RescueWizard.svelte";
  import RequestTimeline from "../timeline/RequestTimeline.svelte";

  let { views }: { views: GitViews } = $props();

  const titles: Partial<Record<GitView["kind"], string>> = {
    diff: "Changes",
    merge: "Merge",
    compare: "Compare revisions",
    bisect: "Bisect",
    timeline: "Request history",
    contract: "API contract",
    rescue: "Secret rescue",
  };
  const icons: Partial<Record<GitView["kind"], string>> = {
    diff: "compare",
    merge: "merge",
    compare: "compare",
    bisect: "bisect",
    timeline: "history",
    contract: "contract",
    rescue: "shield-secret",
  };
</script>

{#snippet hostView(view: GitView)}
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
  {:else if view.kind === "contract"}
    <ContractPanel />
  {:else}
    <RescueWizard />
  {/if}
{/snippet}

<GitViewHost {views} {titles} {icons} {hostView} />

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 12 interface: shows the open Git-for-APIs view (`views.svelte.ts`) in the main area,
     with a header to close it. Each kind is one strand's component; this file only maps kinds
     to components. -->
<script lang="ts">
  import BisectPanel from "../compare/BisectPanel.svelte";
  import ComparePanel from "../compare/ComparePanel.svelte";
  import ContractPanel from "../contract/ContractPanel.svelte";
  import GraphView from "./graph/GraphView.svelte";
  import ChangesView from "./changes/ChangesView.svelte";
  import FileView from "./files/FileView.svelte";
  import BlameView from "./history/BlameView.svelte";
  import FileHistoryView from "./history/FileHistoryView.svelte";
  import ReflogView from "./history/ReflogView.svelte";
  import SearchView from "./history/SearchView.svelte";
  import TextMergeView from "./conflict/TextMergeView.svelte";
  import ExtensionGitView from "./extension/ExtensionGitView.svelte";
  import { extensionGitView } from "../shell/extension-git";
  import { getExtensions } from "../shell/extension-tabs";
  import SemanticDiffView from "../diff/SemanticDiffView.svelte";
  import MergeView from "../merge/MergeView.svelte";
  import RescueWizard from "../rescue/RescueWizard.svelte";
  import ReviewPanel from "../review/ReviewPanel.svelte";
  import RequestTimeline from "../timeline/RequestTimeline.svelte";
  import { Icon, IconButton } from "../ui";
  import type { GitView, GitViews } from "./views.svelte";

  let { views }: { views: GitViews } = $props();
  const extensions = getExtensions();

  const TITLES: Record<GitView["kind"], string> = {
    diff: "Changes",
    merge: "Merge",
    compare: "Compare revisions",
    bisect: "Bisect",
    timeline: "Request history",
    review: "Pull requests",
    contract: "API contract",
    rescue: "Secret rescue",
    graph: "History",
    changes: "Changes",
    file: "File",
    "file-history": "File history",
    blame: "Blame",
    search: "Search history",
    reflog: "Lost work",
    "text-merge": "Resolve conflict",
    extension: "Git",
  };
  const ICONS: Record<GitView["kind"], string> = {
    diff: "compare",
    merge: "merge",
    compare: "compare",
    bisect: "bisect",
    timeline: "history",
    review: "review",
    contract: "contract",
    rescue: "shield-secret",
    graph: "graph",
    changes: "commit",
    file: "file-text",
    "file-history": "history",
    blame: "history",
    search: "search",
    reflog: "undo",
    "text-merge": "merge",
    extension: "graph",
  };
  // An extension's view names its own title and icon.
  const extensionView = $derived(views.current?.kind === "extension" ? extensionGitView(extensions, views.current.id) : null);
  const title = $derived(views.current ? (extensionView?.title ?? TITLES[views.current.kind]) : "");
  const icon = $derived(views.current ? (extensionView?.icon ?? ICONS[views.current.kind]) : "graph");

  function onkeydown(e: KeyboardEvent) {
    // Escape closes the view unless something inside (a picker, a menu, a field) used it.
    if (e.key !== "Escape" || e.defaultPrevented) return;
    const t = e.target as HTMLElement | null;
    if (t?.closest("input, textarea, select, [role='listbox'], [role='menu'], [role='dialog']")) return;
    views.close();
  }
</script>

{#if views.current}
  {@const view = views.current}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <section class="git-view" data-testid="git-view" data-kind={view.kind} aria-label={title} {onkeydown}>
    <header>
      <span class="icon"><Icon name={icon} /></span>
      <h2>{title}</h2>
      <IconButton icon="close" label="Close view" shortcut="Escape" tooltipPlacement="left" data-testid="git-view-close" onclick={() => views.close()} />
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
      {:else if view.kind === "graph"}
        <GraphView select={view.select} />
      {:else if view.kind === "changes"}
        <ChangesView />
      {:else if view.kind === "file"}
        <FileView path={view.path} rev={view.rev} />
      {:else if view.kind === "file-history"}
        <FileHistoryView path={view.path} />
      {:else if view.kind === "blame"}
        <BlameView path={view.path} rev={view.rev} />
      {:else if view.kind === "search"}
        <SearchView query={view.query} />
      {:else if view.kind === "reflog"}
        <ReflogView name={view.name} />
      {:else if view.kind === "text-merge"}
        <TextMergeView path={view.path} onclose={() => views.close()} />
      {:else if view.kind === "extension"}
        <ExtensionGitView id={view.id} props={view.props ?? {}} onclose={() => views.close()} />
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
    gap: var(--rt-space-4);
    flex: none;
    height: var(--rt-bar-height);
    padding: 0 var(--rt-space-4) 0 var(--rt-space-6);
    border-bottom: 1px solid var(--rt-border);
    background: var(--rt-surface-1);
  }
  .icon {
    display: inline-flex;
    color: var(--rt-fg-muted);
  }
  h2 {
    flex: 1;
    min-width: 0;
    margin: 0;
    font: var(--rt-font-title);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--rt-surface-1);
  }
</style>

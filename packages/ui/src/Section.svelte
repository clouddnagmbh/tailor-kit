<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A titled group inside a panel (sidebar parts, settings groups): header with optional count
     and actions, optionally collapsible (the header is the toggle, aria-expanded). -->
<script lang="ts">
  import type { Snippet } from "svelte";

  import Icon from "./Icon.svelte";

  let {
    title,
    description,
    count,
    collapsible = false,
    open = $bindable(true),
    actions,
    children,
  }: {
    title: string;
    description?: string;
    count?: number;
    collapsible?: boolean;
    open?: boolean;
    actions?: Snippet;
    children: Snippet;
  } = $props();

  const id = $props.id();
</script>

<section class="rt-section" class:closed={collapsible && !open}>
  <header>
    {#if collapsible}
      <button type="button" class="toggle" aria-expanded={open} aria-controls="{id}-body" onclick={() => (open = !open)}>
        <Icon name="chevron-right" size="sm" class="chev" />
        <span class="title">{title}</span>
        {#if count !== undefined}<span class="count">{count}</span>{/if}
      </button>
    {:else}
      <h3 class="title">{title}</h3>
      {#if count !== undefined}<span class="count">{count}</span>{/if}
    {/if}
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </header>
  {#if description && (!collapsible || open)}<p class="description">{description}</p>{/if}
  {#if !collapsible || open}
    <div class="body" id="{id}-body">{@render children()}</div>
  {/if}
</section>

<style>
  .rt-section {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    min-height: var(--rt-control-sm);
  }
  .title {
    margin: 0;
    color: var(--rt-fg-muted);
    font: var(--rt-weight-semibold) var(--rt-text-caption) / var(--rt-leading-caption) var(--rt-font-ui);
    letter-spacing: var(--rt-tracking-caps);
    text-transform: uppercase;
  }
  .toggle {
    display: inline-flex;
    flex: 1;
    align-items: center;
    gap: var(--rt-space-2);
    min-width: 0;
    height: var(--rt-control-sm);
    margin-left: calc(-1 * var(--rt-space-2));
    padding: 0 var(--rt-space-2);
    border: 0;
    border-radius: var(--rt-radius-sm);
    background: transparent;
    color: var(--rt-fg-muted);
    text-align: left;
    cursor: pointer;
  }
  .toggle:hover {
    color: var(--rt-fg);
  }
  .toggle:hover .title {
    color: var(--rt-fg);
  }
  .toggle:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
  }
  .toggle :global(.chev) {
    transform: rotate(90deg);
    transition: transform var(--rt-dur-fast) var(--rt-ease);
  }
  .closed .toggle :global(.chev) {
    transform: none;
  }
  .count {
    padding: 0 var(--rt-space-2);
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-sunken);
    color: var(--rt-fg-muted);
    font: var(--rt-weight-medium) var(--rt-text-caption) / 16px var(--rt-font-ui);
    font-variant-numeric: tabular-nums;
  }
  .actions {
    display: flex;
    gap: var(--rt-space-1);
    margin-left: auto;
  }
  .description {
    margin: 0;
    color: var(--rt-fg-muted);
  }
</style>

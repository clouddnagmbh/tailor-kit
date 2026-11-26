<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A row of tabs (WAI-ARIA tabs, automatic activation): arrows move and select, Home/End jump.
     The panel is yours: give it `role="tabpanel"`, `id={tabPanelId(idPrefix, id)}` and
     `aria-labelledby={tabId(idPrefix, id)}`. -->
<script module lang="ts">
  export interface TabItem {
    id: string;
    label: string;
    icon?: string;
    /** A count after the label. */
    count?: number;
    disabled?: boolean;
  }
  export const tabId = (prefix: string, id: string) => `${prefix}-tab-${id}`;
  export const tabPanelId = (prefix: string, id: string) => `${prefix}-panel-${id}`;
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import { rove } from "./roving";

  let {
    items,
    value = $bindable(),
    label,
    idPrefix,
    size = "md",
    onchange,
  }: {
    items: TabItem[];
    value: string;
    /** Accessible name of the tab list. */
    label: string;
    idPrefix?: string;
    size?: "sm" | "md";
    onchange?: (id: string) => void;
  } = $props();

  const uid = $props.id();
  const prefix = $derived(idPrefix ?? uid);
  const buttons: HTMLButtonElement[] = $state([]);

  function select(i: number) {
    const item = items[i];
    if (!item || item.disabled) return;
    if (value !== item.id) {
      value = item.id;
      onchange?.(item.id);
    }
    buttons[i]?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    const current = items.findIndex((t) => t.id === value);
    const next = rove(
      items.map((t) => !t.disabled),
      current,
      e.key,
      "horizontal",
    );
    if (next === null) return;
    e.preventDefault();
    select(next);
  }
</script>

<div class="rt-tabs {size}" role="tablist" aria-label={label} tabindex="-1" {onkeydown}>
  {#each items as item, i (item.id)}
    <button
      bind:this={buttons[i]}
      type="button"
      role="tab"
      id={tabId(prefix, item.id)}
      aria-controls={tabPanelId(prefix, item.id)}
      aria-selected={value === item.id}
      tabindex={value === item.id ? 0 : -1}
      disabled={item.disabled}
      onclick={() => select(i)}
    >
      {#if item.icon}<Icon name={item.icon} size="sm" />{/if}
      <span>{item.label}</span>
      {#if item.count !== undefined}<span class="count">{item.count}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .rt-tabs {
    display: flex;
    gap: var(--rt-space-2);
    min-width: 0;
    border-bottom: 1px solid var(--rt-border);
    overflow-x: auto;
    scrollbar-width: none;
    outline: none;
  }
  button {
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--rt-space-3);
    height: var(--rt-control-lg);
    padding: 0 var(--rt-space-4);
    border: 0;
    background: transparent;
    color: var(--rt-fg-muted);
    font: var(--rt-weight-medium) var(--rt-text-body) / 1 var(--rt-font-ui);
    cursor: pointer;
    transition: color var(--rt-dur-fast) var(--rt-ease);
  }
  .sm button {
    height: var(--rt-control-md);
    padding: 0 var(--rt-space-3);
    font-size: var(--rt-text-small);
  }
  button::after {
    content: "";
    position: absolute;
    left: var(--rt-space-3);
    right: var(--rt-space-3);
    bottom: -1px;
    height: 2px;
    border-radius: 2px 2px 0 0;
    background: transparent;
    transition: background-color var(--rt-dur-fast) var(--rt-ease);
  }
  button:hover:not(:disabled) {
    color: var(--rt-fg);
  }
  button[aria-selected="true"] {
    color: var(--rt-fg);
  }
  button[aria-selected="true"]::after {
    background: var(--rt-accent);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 var(--rt-focus-width) var(--rt-focus);
    border-radius: var(--rt-radius-sm);
  }
  .count {
    padding: 0 var(--rt-space-2);
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-sunken);
    color: var(--rt-fg-muted);
    font: var(--rt-weight-semibold) var(--rt-text-caption) / 16px var(--rt-font-ui);
    font-variant-numeric: tabular-nums;
  }
  button[aria-selected="true"] .count {
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
</style>

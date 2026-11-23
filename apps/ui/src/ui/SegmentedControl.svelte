<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A choice of two to five options shown at once (view modes, "Side by side | Unified").
     A radio group: arrows move and select. Options can be icon-only (then `title` is required). -->
<script module lang="ts">
  export interface SegmentOption {
    value: string;
    label?: string;
    icon?: string;
    /** Accessible name and tooltip for icon-only options. */
    title?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import { rove } from "./roving";
  import { tooltip } from "./tooltip";

  let {
    options,
    value = $bindable(),
    label,
    size = "md",
    onchange,
  }: {
    options: SegmentOption[];
    value: string;
    label: string;
    size?: "sm" | "md";
    onchange?: (value: string) => void;
  } = $props();

  const buttons: HTMLButtonElement[] = $state([]);

  function select(i: number) {
    const o = options[i];
    if (!o || o.disabled) return;
    if (value !== o.value) {
      value = o.value;
      onchange?.(o.value);
    }
    buttons[i]?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    const current = options.findIndex((o) => o.value === value);
    const next = rove(
      options.map((o) => !o.disabled),
      current,
      e.key === "ArrowDown" ? "ArrowRight" : e.key === "ArrowUp" ? "ArrowLeft" : e.key,
      "horizontal",
    );
    if (next === null) return;
    e.preventDefault();
    select(next);
  }
</script>

<div class="rt-segmented {size}" role="radiogroup" aria-label={label} tabindex="-1" {onkeydown}>
  {#each options as o, i (o.value)}
    <button
      bind:this={buttons[i]}
      type="button"
      role="radio"
      aria-checked={value === o.value}
      aria-label={o.label ? undefined : o.title}
      tabindex={value === o.value ? 0 : -1}
      disabled={o.disabled}
      class:icon-only={!o.label}
      use:tooltip={{ text: o.label ? "" : (o.title ?? ""), describe: false }}
      onclick={() => select(i)}
    >
      {#if o.icon}<Icon name={o.icon} size="sm" />{/if}
      {#if o.label}<span>{o.label}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .rt-segmented {
    --seg-h: var(--rt-control-md);
    display: inline-flex;
    flex: none;
    gap: var(--rt-space-1);
    height: var(--seg-h);
    box-sizing: border-box;
    padding: var(--rt-space-1);
    border-radius: var(--rt-radius-md);
    background: var(--rt-surface-sunken);
    box-shadow: inset 0 0 0 1px var(--rt-border);
    outline: none;
  }
  .sm {
    --seg-h: var(--rt-control-sm);
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--rt-space-2);
    min-width: 0;
    padding: 0 var(--rt-space-4);
    border: 0;
    border-radius: calc(var(--rt-radius-md) - 2px);
    background: transparent;
    color: var(--rt-fg-muted);
    font: var(--rt-weight-medium) var(--rt-text-small) / 1 var(--rt-font-ui);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--rt-dur-fast) var(--rt-ease),
      color var(--rt-dur-fast) var(--rt-ease),
      box-shadow var(--rt-dur-fast) var(--rt-ease);
  }
  .sm button {
    padding: 0 var(--rt-space-3);
    font-size: var(--rt-text-caption);
  }
  .icon-only {
    padding: 0;
    aspect-ratio: 1.25;
  }
  button:hover:not(:disabled) {
    color: var(--rt-fg);
  }
  button[aria-checked="true"] {
    background: var(--rt-surface-3);
    color: var(--rt-fg);
    box-shadow:
      var(--rt-elev-1),
      0 0 0 1px var(--rt-border);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: 1px;
  }
</style>

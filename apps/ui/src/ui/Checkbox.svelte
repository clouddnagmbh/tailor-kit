<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A checkbox with label and optional description; `indeterminate` for "some selected". -->
<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";

  import Icon from "./Icon.svelte";

  let {
    checked = $bindable(false),
    indeterminate = false,
    label,
    description,
    disabled = false,
    ...rest
  }: Omit<HTMLInputAttributes, "type" | "checked" | "class"> & {
    checked?: boolean;
    indeterminate?: boolean;
    label?: string;
    description?: string;
  } = $props();

  let input = $state<HTMLInputElement>();
  $effect(() => {
    if (input) input.indeterminate = indeterminate;
  });
</script>

<label class="rt-checkbox" class:disabled>
  <span class="box">
    <input bind:this={input} type="checkbox" bind:checked {disabled} {...rest} />
    <span class="mark" aria-hidden="true">
      {#if indeterminate}<Icon name="minus" size="sm" />{:else}<Icon name="check" size="sm" />{/if}
    </span>
  </span>
  {#if label || description}
    <span class="text">
      {#if label}<span class="label">{label}</span>{/if}
      {#if description}<span class="description">{description}</span>{/if}
    </span>
  {/if}
</label>

<style>
  .rt-checkbox {
    display: inline-flex;
    align-items: flex-start;
    gap: var(--rt-space-4);
    color: var(--rt-fg);
    font: var(--rt-font-body);
    cursor: pointer;
  }
  .disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .box {
    position: relative;
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 16px;
    height: 16px;
    margin-top: 1px;
  }
  input {
    appearance: none;
    width: 16px;
    height: 16px;
    margin: 0;
    border: 1px solid var(--rt-border-strong);
    border-radius: var(--rt-radius-sm);
    background: var(--rt-surface-2);
    box-shadow: var(--rt-elev-1);
    cursor: inherit;
    transition:
      background-color var(--rt-dur-fast) var(--rt-ease),
      border-color var(--rt-dur-fast) var(--rt-ease);
  }
  input:hover:not(:disabled) {
    border-color: var(--rt-fg-subtle);
  }
  input:checked,
  input:indeterminate {
    border-color: var(--rt-accent-solid);
    background: var(--rt-accent-solid);
  }
  input:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .mark {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--rt-accent-solid-fg);
    opacity: 0;
    transform: scale(0.6);
    pointer-events: none;
    transition:
      opacity var(--rt-dur-fast) var(--rt-ease),
      transform var(--rt-dur-fast) var(--rt-ease);
  }
  .mark :global(.rt-icon) {
    width: 12px;
    height: 12px;
    stroke-width: 3;
  }
  input:checked + .mark,
  input:indeterminate + .mark {
    opacity: 1;
    transform: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-1);
  }
  .label {
    line-height: var(--rt-leading-body);
  }
  .description {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
</style>

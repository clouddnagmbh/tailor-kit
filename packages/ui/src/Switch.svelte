<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- An on/off setting that applies at once (role="switch"). For choices that are submitted
     later, use Checkbox. -->
<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";

  let {
    checked = $bindable(false),
    label,
    description,
    size = "md",
    disabled = false,
    ...rest
  }: Omit<HTMLInputAttributes, "type" | "checked" | "class" | "size"> & {
    checked?: boolean;
    label?: string;
    description?: string;
    size?: "sm" | "md";
  } = $props();
</script>

<label class="rt-switch {size}" class:disabled>
  <input type="checkbox" role="switch" bind:checked {disabled} aria-checked={checked} {...rest} />
  {#if label || description}
    <span class="text">
      {#if label}<span class="label">{label}</span>{/if}
      {#if description}<span class="description">{description}</span>{/if}
    </span>
  {/if}
</label>

<style>
  .rt-switch {
    --w: 30px;
    --h: 18px;
    display: inline-flex;
    align-items: flex-start;
    gap: var(--rt-space-4);
    color: var(--rt-fg);
    font: var(--rt-font-body);
    cursor: pointer;
  }
  .sm {
    --w: 24px;
    --h: 14px;
  }
  .disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  input {
    position: relative;
    appearance: none;
    flex: none;
    width: var(--w);
    height: var(--h);
    margin: 0;
    border-radius: var(--rt-radius-pill);
    background: var(--rt-switch-off);
    cursor: inherit;
    transition: background-color var(--rt-dur-normal) var(--rt-ease);
  }
  input::before {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: calc(var(--h) - 4px);
    height: calc(var(--h) - 4px);
    border-radius: 50%;
    background: #ffffff;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform var(--rt-dur-normal) var(--rt-ease);
  }
  input:checked {
    background: var(--rt-accent-solid);
  }
  input:checked::before {
    transform: translateX(calc(var(--w) - var(--h)));
  }
  input:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-1);
  }
  .label {
    line-height: var(--rt-leading-body);
  }
  .rt-switch:not(.sm) .text {
    margin-top: 0;
  }
  .description {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
</style>

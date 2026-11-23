<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A styled native <select> (keyboard, screen readers and long lists for free). For actions
     use Dropdown; for two to five visible choices use SegmentedControl. -->
<script module lang="ts">
  export interface SelectOption {
    value: string;
    label: string;
    disabled?: boolean;
  }
</script>

<script lang="ts">
  import type { HTMLSelectAttributes } from "svelte/elements";

  import "./field.css";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  let {
    value = $bindable(""),
    options,
    label,
    hint,
    error,
    icon,
    size = "md",
    id,
    disabled = false,
    ...rest
  }: Omit<HTMLSelectAttributes, "value" | "size" | "class"> & {
    value?: string;
    options: SelectOption[];
    label?: string;
    hint?: string;
    error?: string;
    icon?: IconName | (string & {});
    size?: "sm" | "md" | "lg";
  } = $props();

  const uid = $props.id();
  const inputId = $derived(id ?? `${uid}-input`);
  const describedBy = $derived([error ? `${uid}-error` : "", hint ? `${uid}-hint` : ""].filter(Boolean).join(" "));
</script>

<div class="rt-field">
  {#if label}<label class="rt-field-label" for={inputId}>{label}</label>{/if}
  <div class="rt-field-box {size} select" class:invalid={Boolean(error)} class:disabled>
    {#if icon}<Icon name={icon} size="sm" />{/if}
    <select
      bind:value
      id={inputId}
      class="rt-field-control"
      {disabled}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedBy || undefined}
      {...rest}
    >
      {#each options as o (o.value)}
        <option value={o.value} disabled={o.disabled}>{o.label}</option>
      {/each}
    </select>
    <Icon name="chevron-down" size="sm" class="chev" />
  </div>
  {#if error}<p class="rt-field-error" id="{uid}-error">{error}</p>{/if}
  {#if hint}<p class="rt-field-hint" id="{uid}-hint">{hint}</p>{/if}
</div>

<style>
  .select {
    padding-right: 0;
  }
  select {
    appearance: none;
    padding-right: calc(var(--rt-space-4) + var(--rt-icon-sm) + var(--rt-space-2));
    cursor: pointer;
  }
  .select :global(.chev) {
    position: absolute;
    right: var(--rt-space-4);
    pointer-events: none;
    color: var(--rt-fg-muted);
  }
  .select.sm :global(.chev) {
    right: var(--rt-space-3);
  }
</style>

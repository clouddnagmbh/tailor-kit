<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A one-line text input with optional label, leading icon, trailing content (a Kbd, a clear
     IconButton), hint and error. Extra attributes and handlers go to the <input>. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLInputAttributes } from "svelte/elements";

  import "./field.css";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  let {
    value = $bindable(""),
    label,
    hint,
    error,
    icon,
    size = "md",
    mono = false,
    id,
    disabled = false,
    element = $bindable(),
    trailing,
    ...rest
  }: Omit<HTMLInputAttributes, "size" | "value" | "class"> & {
    value?: string;
    label?: string;
    hint?: string;
    /** An error message; marks the field invalid. */
    error?: string;
    icon?: IconName | (string & {});
    size?: "sm" | "md" | "lg";
    mono?: boolean;
    element?: HTMLInputElement;
    trailing?: Snippet;
  } = $props();

  const uid = $props.id();
  const inputId = $derived(id ?? `${uid}-input`);
  const describedBy = $derived([error ? `${uid}-error` : "", hint ? `${uid}-hint` : ""].filter(Boolean).join(" "));
</script>

<div class="rt-field">
  {#if label}<label class="rt-field-label" for={inputId}>{label}</label>{/if}
  <div class="rt-field-box {size}" class:invalid={Boolean(error)} class:disabled>
    {#if icon}<Icon name={icon} size="sm" />{/if}
    <input
      bind:this={element}
      bind:value
      id={inputId}
      class="rt-field-control"
      class:mono
      {disabled}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedBy || undefined}
      spellcheck={mono ? false : undefined}
      {...rest}
    />
    {#if trailing}{@render trailing()}{/if}
  </div>
  {#if error}<p class="rt-field-error" id="{uid}-error">{error}</p>{/if}
  {#if hint}<p class="rt-field-hint" id="{uid}-hint">{hint}</p>{/if}
</div>

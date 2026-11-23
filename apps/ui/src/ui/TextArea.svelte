<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A multi-line text input (commit messages, descriptions). For code use the CodeMirror
     editor, not this. Extra attributes and handlers go to the <textarea>. -->
<script lang="ts">
  import type { HTMLTextareaAttributes } from "svelte/elements";

  import "./field.css";

  let {
    value = $bindable(""),
    label,
    hint,
    error,
    rows = 4,
    mono = false,
    id,
    disabled = false,
    element = $bindable(),
    ...rest
  }: Omit<HTMLTextareaAttributes, "value" | "class"> & {
    value?: string;
    label?: string;
    hint?: string;
    error?: string;
    mono?: boolean;
    element?: HTMLTextAreaElement;
  } = $props();

  const uid = $props.id();
  const inputId = $derived(id ?? `${uid}-input`);
  const describedBy = $derived([error ? `${uid}-error` : "", hint ? `${uid}-hint` : ""].filter(Boolean).join(" "));
</script>

<div class="rt-field">
  {#if label}<label class="rt-field-label" for={inputId}>{label}</label>{/if}
  <div class="rt-field-box area" class:invalid={Boolean(error)} class:disabled>
    <textarea
      bind:this={element}
      bind:value
      id={inputId}
      class="rt-field-control"
      class:mono
      {rows}
      {disabled}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedBy || undefined}
      {...rest}
    ></textarea>
  </div>
  {#if error}<p class="rt-field-error" id="{uid}-error">{error}</p>{/if}
  {#if hint}<p class="rt-field-hint" id="{uid}-hint">{hint}</p>{/if}
</div>

<style>
  .area {
    align-items: stretch;
    padding: var(--rt-space-3) var(--rt-space-4);
  }
  textarea {
    height: auto;
    min-height: 0;
    resize: vertical;
    line-height: var(--rt-leading-body);
  }
</style>

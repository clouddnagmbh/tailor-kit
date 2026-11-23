<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The button. One primary per view; secondary for the rest; ghost in toolbars and rows;
     danger only for the confirming step of a destructive action. Extra attributes and event
     handlers go to the <button>. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";
  import Spinner from "./Spinner.svelte";

  type Props = Omit<HTMLButtonAttributes, "class"> & {
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "sm" | "md" | "lg";
    icon?: IconName | (string & {});
    iconRight?: IconName | (string & {});
    /** Shows a spinner and blocks clicks; the label stays, so the width does not jump. */
    loading?: boolean;
    /** Stretches to the width of its container. */
    block?: boolean;
    class?: string;
    element?: HTMLButtonElement;
    children?: Snippet;
  };

  let {
    variant = "secondary",
    size = "md",
    icon,
    iconRight,
    loading = false,
    block = false,
    disabled = false,
    type = "button",
    class: className = "",
    element = $bindable(),
    children,
    onclick,
    ...rest
  }: Props = $props();

  const iconSize = $derived(size === "sm" ? "sm" : "md");
</script>

<button
  bind:this={element}
  {type}
  class="rt-btn {variant} {size} {className}"
  class:block
  class:loading
  class:icon-only={!children}
  disabled={disabled || undefined}
  aria-busy={loading || undefined}
  onclick={(e) => {
    if (loading) return e.preventDefault();
    onclick?.(e);
  }}
  {...rest}
>
  {#if loading}
    <Spinner size={iconSize} label="Working" />
  {:else if icon}
    <Icon name={icon} size={iconSize} />
  {/if}
  {#if children}<span class="label">{@render children()}</span>{/if}
  {#if iconRight}<Icon name={iconRight} size={iconSize} class="trail" />{/if}
</button>

<style>
  .rt-btn {
    --btn-h: var(--rt-control-md);
    --btn-px: var(--rt-space-5);
    position: relative;
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: var(--rt-space-3);
    height: var(--btn-h);
    min-width: var(--btn-h);
    padding: 0 var(--btn-px);
    border: 1px solid transparent;
    border-radius: var(--rt-radius-md);
    font: var(--rt-weight-medium) var(--rt-text-body) / 1 var(--rt-font-ui);
    letter-spacing: var(--rt-tracking-tight);
    white-space: nowrap;
    cursor: pointer;
    user-select: none;
    transition:
      background-color var(--rt-dur-fast) var(--rt-ease),
      border-color var(--rt-dur-fast) var(--rt-ease),
      color var(--rt-dur-fast) var(--rt-ease),
      box-shadow var(--rt-dur-fast) var(--rt-ease);
  }
  .sm {
    --btn-h: var(--rt-control-sm);
    --btn-px: var(--rt-space-4);
    gap: var(--rt-space-2);
    font-size: var(--rt-text-small);
  }
  .lg {
    --btn-h: var(--rt-control-lg);
    --btn-px: var(--rt-space-6);
  }
  .icon-only {
    padding: 0;
    width: var(--btn-h);
  }
  .block {
    display: flex;
    width: 100%;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .primary {
    background: var(--rt-accent-solid);
    color: var(--rt-accent-solid-fg);
    box-shadow:
      var(--rt-elev-1),
      inset 0 1px 0 rgb(255 255 255 / 0.14);
  }
  .primary:hover:not(:disabled) {
    background: var(--rt-accent-solid-hover);
  }
  .secondary {
    background: var(--rt-surface-2);
    border-color: var(--rt-border-strong);
    color: var(--rt-fg);
    box-shadow: var(--rt-elev-1);
  }
  .secondary:hover:not(:disabled) {
    background: var(--rt-bg-hover);
  }
  .secondary:active:not(:disabled) {
    background: var(--rt-bg-active);
  }
  .ghost {
    background: transparent;
    color: var(--rt-fg-muted);
  }
  .ghost:hover:not(:disabled) {
    background: var(--rt-bg-hover);
    color: var(--rt-fg);
  }
  .ghost:active:not(:disabled) {
    background: var(--rt-bg-active);
  }
  .danger {
    background: var(--rt-danger-solid);
    color: var(--rt-fg-on-solid);
    box-shadow:
      var(--rt-elev-1),
      inset 0 1px 0 rgb(255 255 255 / 0.14);
  }
  .danger:hover:not(:disabled) {
    background: var(--rt-danger-solid-hover);
  }

  .rt-btn:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .rt-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    box-shadow: none;
  }
  .loading {
    cursor: progress;
  }
  .rt-btn :global(.trail) {
    margin-right: calc(-1 * var(--rt-space-1));
    opacity: 0.8;
  }
</style>

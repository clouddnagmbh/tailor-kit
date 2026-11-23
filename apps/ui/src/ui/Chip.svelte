<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A pill: a filter, a ref name, an environment. With `onclick` or `caret` it is a button
     (`caret` for a Dropdown trigger); with `onremove` it gets a remove button. -->
<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";

  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  type Tone = "neutral" | "accent" | "info" | "ok" | "warn" | "danger";

  let {
    label,
    icon,
    tone = "neutral",
    selected = false,
    caret = false,
    size = "md",
    mono = false,
    onclick,
    onremove,
    removeLabel,
    ...rest
  }: Omit<HTMLButtonAttributes, "class" | "children"> & {
    label: string;
    icon?: IconName | (string & {});
    tone?: Tone;
    selected?: boolean;
    caret?: boolean;
    size?: "sm" | "md";
    /** Monospace label (ref names, hashes). */
    mono?: boolean;
    onremove?: () => void;
    removeLabel?: string;
  } = $props();

  const interactive = $derived(Boolean(onclick) || caret || rest["aria-haspopup"] !== undefined);
</script>

{#snippet body()}
  {#if icon}<Icon name={icon} size="sm" />{/if}
  <span class="label" class:mono>{label}</span>
  {#if caret}<Icon name="chevron-down" size="sm" class="caret" />{/if}
{/snippet}

<span class="rt-chip t-{tone} {size}" class:selected class:interactive>
  {#if interactive}
    <button type="button" class="hit" aria-pressed={onclick && !caret ? selected : undefined} {onclick} {...rest}>
      {@render body()}
    </button>
  {:else}
    <span class="hit">{@render body()}</span>
  {/if}
  {#if onremove}
    <button type="button" class="remove" aria-label={removeLabel ?? `Remove ${label}`} onclick={onremove}>
      <Icon name="close" size="sm" />
    </button>
  {/if}
</span>

<style>
  .rt-chip {
    --chip-h: var(--rt-control-sm);
    display: inline-flex;
    flex: none;
    align-items: center;
    max-width: 100%;
    height: var(--chip-h);
    box-sizing: border-box;
    border: 1px solid var(--rt-border-strong);
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-2);
    color: var(--rt-fg);
    font: var(--rt-weight-medium) var(--rt-text-small) / 1 var(--rt-font-ui);
    transition:
      background-color var(--rt-dur-fast) var(--rt-ease),
      border-color var(--rt-dur-fast) var(--rt-ease);
  }
  .sm {
    --chip-h: 20px;
    font-size: var(--rt-text-caption);
  }
  .hit {
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-2);
    min-width: 0;
    height: 100%;
    padding: 0 var(--rt-space-4);
    border: 0;
    border-radius: inherit;
    background: transparent;
    color: inherit;
    font: inherit;
  }
  .sm .hit {
    padding: 0 var(--rt-space-3);
  }
  button.hit {
    cursor: pointer;
  }
  .hit :global(.rt-icon) {
    color: var(--rt-fg-muted);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mono {
    font-family: var(--rt-font-mono);
    font-weight: var(--rt-weight-regular);
  }
  .hit :global(.caret) {
    margin-right: calc(-1 * var(--rt-space-1));
  }
  .interactive:hover {
    background: var(--rt-bg-hover);
  }
  .selected,
  .selected:hover {
    border-color: color-mix(in srgb, var(--rt-accent) 45%, transparent);
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .selected .hit :global(.rt-icon) {
    color: currentColor;
  }
  .t-accent {
    border-color: transparent;
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .t-info {
    border-color: var(--rt-info-border);
    background: var(--rt-info-bg);
    color: var(--rt-info-fg);
  }
  .t-ok {
    border-color: var(--rt-ok-border);
    background: var(--rt-ok-bg);
    color: var(--rt-ok-fg);
  }
  .t-warn {
    border-color: var(--rt-warn-border);
    background: var(--rt-warn-bg);
    color: var(--rt-warn-fg);
  }
  .t-danger {
    border-color: var(--rt-danger-border);
    background: var(--rt-danger-bg);
    color: var(--rt-danger-fg);
  }
  .t-accent .hit :global(.rt-icon),
  .t-info .hit :global(.rt-icon),
  .t-ok .hit :global(.rt-icon),
  .t-warn .hit :global(.rt-icon),
  .t-danger .hit :global(.rt-icon) {
    color: currentColor;
  }
  .remove {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    margin: 0 var(--rt-space-1) 0 calc(-1 * var(--rt-space-2));
    padding: 0;
    border: 0;
    border-radius: var(--rt-radius-pill);
    background: transparent;
    color: var(--rt-fg-muted);
    cursor: pointer;
  }
  .remove:hover {
    background: var(--rt-bg-active);
    color: var(--rt-fg);
  }
  .hit:focus-visible,
  .remove:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: 1px;
  }
</style>

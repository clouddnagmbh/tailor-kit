<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A square button with only an icon. `label` is required: it is the accessible name and the
     tooltip. `pressed` makes it a toggle (aria-pressed). -->
<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";

  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";
  import type { Placement } from "./position";
  import { tooltip } from "./tooltip";

  type Props = Omit<HTMLButtonAttributes, "class" | "children"> & {
    icon: IconName | (string & {});
    label: string;
    /** Shortcut shown in the tooltip (`Mod+K`). */
    shortcut?: string;
    variant?: "ghost" | "secondary" | "primary" | "danger";
    size?: "sm" | "md" | "lg";
    pressed?: boolean;
    tooltipPlacement?: Placement;
    class?: string;
    element?: HTMLButtonElement;
  };

  let {
    icon,
    label,
    shortcut,
    variant = "ghost",
    size = "md",
    pressed,
    tooltipPlacement = "top",
    disabled = false,
    type = "button",
    class: className = "",
    element = $bindable(),
    ...rest
  }: Props = $props();
</script>

<button
  bind:this={element}
  {type}
  class="rt-icon-btn {variant} {size} {className}"
  class:pressed
  aria-label={label}
  aria-pressed={pressed}
  disabled={disabled || undefined}
  use:tooltip={{ text: label, shortcut, placement: tooltipPlacement, describe: false }}
  {...rest}
>
  <Icon name={icon} size={size === "lg" ? "lg" : size === "sm" ? "sm" : "md"} />
</button>

<style>
  .rt-icon-btn {
    --h: var(--rt-control-md);
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: var(--h);
    height: var(--h);
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--rt-radius-md);
    background: transparent;
    color: var(--rt-fg-muted);
    cursor: pointer;
    transition:
      background-color var(--rt-dur-fast) var(--rt-ease),
      color var(--rt-dur-fast) var(--rt-ease);
  }
  .sm {
    --h: var(--rt-control-sm);
    border-radius: var(--rt-radius-sm);
  }
  .lg {
    --h: var(--rt-control-lg);
  }
  .ghost:hover:not(:disabled) {
    background: var(--rt-bg-hover);
    color: var(--rt-fg);
  }
  .ghost:active:not(:disabled) {
    background: var(--rt-bg-active);
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
  .primary {
    background: var(--rt-accent-solid);
    color: var(--rt-accent-solid-fg);
  }
  .primary:hover:not(:disabled) {
    background: var(--rt-accent-solid-hover);
  }
  .danger {
    color: var(--rt-danger);
  }
  .danger:hover:not(:disabled) {
    background: var(--rt-danger-bg);
  }
  .pressed,
  .pressed:hover:not(:disabled) {
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .rt-icon-btn:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .rt-icon-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A button with a main action and a menu of related ones ("Commit" | "Commit and push…"). -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";
  import Menu from "./Menu.svelte";
  import type { MenuEntry, MenuItem } from "./menu";

  let {
    items,
    icon,
    variant = "primary",
    size = "md",
    disabled = false,
    loading = false,
    menuLabel = "More options",
    onclick,
    onselect,
    children,
    ...rest
  }: Omit<HTMLButtonAttributes, "class" | "children" | "onclick" | "disabled"> & {
    items: MenuEntry[];
    icon?: IconName | (string & {});
    variant?: "primary" | "secondary";
    size?: "sm" | "md";
    disabled?: boolean;
    loading?: boolean;
    /** Accessible name of the menu button. */
    menuLabel?: string;
    onclick?: (e: MouseEvent) => void;
    onselect?: (item: MenuItem) => void;
    children: Snippet;
  } = $props();

  let open = $state(false);
  let caret = $state<HTMLButtonElement>();
  let initial = $state<"first" | "last">("first");

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      initial = e.key === "ArrowUp" ? "last" : "first";
      open = true;
    }
  }
</script>

<span class="rt-split {variant} {size}" class:disabled>
  <Button {...rest} {variant} {size} {icon} {disabled} {loading} {onclick} class="main">{@render children()}</Button>
  <button
    bind:this={caret}
    type="button"
    class="caret"
    aria-label={menuLabel}
    aria-haspopup="menu"
    aria-expanded={open}
    disabled={disabled || loading || undefined}
    onclick={() => {
      initial = "first";
      open = !open;
    }}
    {onkeydown}
  >
    <Icon name="chevron-down" size="sm" />
  </button>
</span>
<Menu {items} anchor={caret} bind:open placement="bottom-end" {initial} label={menuLabel} {onselect} />

<style>
  .rt-split {
    display: inline-flex;
    flex: none;
    border-radius: var(--rt-radius-md);
    box-shadow: var(--rt-elev-1);
  }
  .rt-split :global(.main) {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    box-shadow: none;
  }
  .caret {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--rt-control-md);
    height: var(--rt-control-md);
    padding: 0;
    border: 1px solid transparent;
    border-left: 0;
    border-radius: 0 var(--rt-radius-md) var(--rt-radius-md) 0;
    cursor: pointer;
    transition: background-color var(--rt-dur-fast) var(--rt-ease);
  }
  .sm .caret {
    width: 22px;
    height: var(--rt-control-sm);
  }
  .primary .caret {
    background: var(--rt-accent-solid);
    color: var(--rt-accent-solid-fg);
    box-shadow:
      inset 1px 0 0 rgb(255 255 255 / 0.22),
      inset 0 1px 0 rgb(255 255 255 / 0.14);
  }
  .primary .caret:hover:not(:disabled) {
    background: var(--rt-accent-solid-hover);
  }
  .secondary .caret {
    background: var(--rt-surface-2);
    border-color: var(--rt-border-strong);
    border-left: 1px solid var(--rt-border-strong);
    color: var(--rt-fg-muted);
  }
  .secondary .caret:hover:not(:disabled) {
    background: var(--rt-bg-hover);
    color: var(--rt-fg);
  }
  .rt-split.secondary :global(.main) {
    border-right: 0;
  }
  .caret:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .caret:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .disabled {
    box-shadow: none;
  }
</style>

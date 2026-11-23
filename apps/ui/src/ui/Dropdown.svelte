<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A trigger that opens a Menu. Default trigger: a Button with a caret. A custom trigger is a
     snippet that gets the ARIA attributes to spread on its button (clicks and keys are handled
     here): `{#snippet trigger(props)}<Chip {...props} caret label="main" />{/snippet}`.
     ArrowDown, Enter and Space open on the first item, ArrowUp on the last. -->
<script module lang="ts">
  export interface TriggerProps {
    "aria-haspopup": "menu";
    "aria-expanded": boolean;
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  import Button from "./Button.svelte";
  import type { IconName } from "./icons";
  import Menu from "./Menu.svelte";
  import type { MenuEntry, MenuItem } from "./menu";
  import type { Placement } from "./position";

  let {
    items,
    label = "",
    icon,
    variant = "secondary",
    size = "md",
    placement = "bottom-start",
    menuLabel,
    disabled = false,
    open = $bindable(false),
    trigger,
    onselect,
  }: {
    items: MenuEntry[];
    /** Text of the default trigger. */
    label?: string;
    icon?: IconName | (string & {});
    variant?: "primary" | "secondary" | "ghost";
    size?: "sm" | "md";
    placement?: Placement;
    menuLabel?: string;
    disabled?: boolean;
    open?: boolean;
    trigger?: Snippet<[TriggerProps]>;
    onselect?: (item: MenuItem) => void;
  } = $props();

  let wrap = $state<HTMLElement>();
  let initial = $state<"first" | "last">("first");
  const button = $derived(wrap?.querySelector<HTMLElement>("button, [tabindex]") ?? null);

  function toggle() {
    if (disabled) return;
    initial = "first";
    open = !open;
  }

  function onkeydown(e: KeyboardEvent) {
    if (disabled || open) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      initial = e.key === "ArrowUp" ? "last" : "first";
      open = true;
    }
  }
</script>

<!-- The wrapper only routes clicks and keys of the trigger inside it; the trigger is the control. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<span class="rt-dropdown" bind:this={wrap} onclick={toggle} {onkeydown}>
  {#if trigger}
    {@render trigger({ "aria-haspopup": "menu", "aria-expanded": open })}
  {:else}
    <Button {variant} {size} {icon} iconRight="chevron-down" {disabled} aria-haspopup="menu" aria-expanded={open}>{label}</Button>
  {/if}
</span>
<Menu {items} anchor={button} bind:open {placement} {initial} label={menuLabel ?? label} {onselect} />

<style>
  .rt-dropdown {
    display: inline-flex;
  }
</style>

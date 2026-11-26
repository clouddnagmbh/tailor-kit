<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A menu (WAI-ARIA menu pattern) in a portal next to `anchor` (an element, or a point for
     context menus). Keyboard model in menu.ts. Closes on outside press, Escape, Tab and after
     a choice; Escape returns focus to the anchor. Usually opened through `Dropdown`. -->
<script lang="ts">
  import { tick } from "svelte";

  import { portal, rectOf, viewport } from "./floating";
  import Icon from "./Icon.svelte";
  import Kbd from "./Kbd.svelte";
  import { enabledMask, isItem, isSeparator, menuKey, type MenuEntry, type MenuItem } from "./menu";
  import { place, type Placement } from "./position";
  import { firstEnabled, lastEnabled } from "./roving";
  import Self from "./Menu.svelte";

  let {
    items,
    anchor,
    open = $bindable(false),
    placement = "bottom-start",
    label,
    initial = "first",
    onselect,
    onclose,
    nested = false,
    root,
  }: {
    items: MenuEntry[];
    anchor: HTMLElement | { x: number; y: number } | null | undefined;
    open?: boolean;
    placement?: Placement;
    /** Accessible name of the menu. */
    label?: string;
    /** Which item has focus when it opens. */
    initial?: "first" | "last";
    /** After a choice (the item's `run` ran already). */
    onselect?: (item: MenuItem) => void;
    /** Called once the menu closed (any reason); `restoreFocus` was done already. */
    onclose?: () => void;
    /** Internal: a submenu. */
    nested?: boolean;
    root?: string;
  } = $props();

  const uid = $props.id();
  const rootId = $derived(root ?? uid);
  let el = $state<HTMLElement>();
  let active = $state(-1);
  let submenu = $state(-1);
  let pos = $state<{ x: number; y: number } | null>(null);
  const buttons: HTMLElement[] = $state([]);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;

  const anchorEl = $derived(anchor instanceof HTMLElement ? anchor : null);

  $effect(() => {
    if (!open || !el || !anchor) return;
    const r = el.getBoundingClientRect();
    pos = place(rectOf(anchor), { width: r.width, height: r.height }, viewport(), placement, nested ? -4 : 4);
  });

  $effect(() => {
    if (!open) return;
    const mask = enabledMask(items);
    active = initial === "last" ? lastEnabled(mask) : firstEnabled(mask);
    submenu = -1;
    tick().then(() => (active >= 0 ? buttons[active] : el)?.focus());
    return () => {
      pos = null;
      clearTimeout(hoverTimer);
    };
  });

  // Outside presses close the whole menu tree (only the root listens).
  $effect(() => {
    if (!open || nested) return;
    const down = (e: PointerEvent) => {
      const t = e.target as Element | null;
      if (t?.closest?.(`[data-rt-menu="${rootId}"]`)) return;
      if (anchorEl?.contains(t)) return;
      close(false);
    };
    const blur = () => close(false);
    document.addEventListener("pointerdown", down, true);
    window.addEventListener("blur", blur);
    return () => {
      document.removeEventListener("pointerdown", down, true);
      window.removeEventListener("blur", blur);
    };
  });

  function close(restoreFocus: boolean) {
    if (!open) return;
    open = false;
    if (restoreFocus) anchorEl?.focus();
    onclose?.();
  }

  function focusAt(i: number) {
    if (i < 0) return;
    active = i;
    buttons[i]?.focus();
  }

  function activate(i: number) {
    const entry = items[i];
    if (!entry || !isItem(entry) || entry.disabled) return;
    if (entry.items?.length) submenu = i;
    else pick(entry);
  }

  /** A choice: a submenu hands it up; the root closes the tree first (focus back on the
   *  trigger before the action runs, which may open a dialog), then runs it. */
  function pick(entry: MenuItem) {
    if (nested) return onselect?.(entry);
    close(true);
    entry.run?.();
    onselect?.(entry);
  }

  function onkeydown(e: KeyboardEvent) {
    const action = menuKey(items, active, e.key, nested);
    if (action.type === "none") return;
    e.preventDefault();
    e.stopPropagation();
    switch (action.type) {
      case "focus":
        submenu = -1;
        focusAt(action.index);
        break;
      case "activate":
        activate(action.index);
        break;
      case "open-submenu":
        submenu = action.index;
        break;
      case "close-submenu":
        close(false);
        break;
      case "close":
        close(action.restoreFocus);
        break;
    }
  }

  function hover(i: number) {
    const entry = items[i];
    if (!entry || !isItem(entry) || entry.disabled) return;
    if (active !== i) focusAt(i);
    clearTimeout(hoverTimer);
    if (entry.items?.length) hoverTimer = setTimeout(() => (submenu = i), 120);
    else if (submenu !== -1) hoverTimer = setTimeout(() => (submenu = -1), 200);
  }

  function closedSubmenu(i: number) {
    submenu = -1;
    focusAt(i);
  }
</script>

{#if open}
  <div
    bind:this={el}
    use:portal={anchorEl}
    class="rt-menu"
    class:placed={pos !== null}
    role="menu"
    tabindex="-1"
    aria-label={label}
    data-rt-menu={rootId}
    style:left="{pos?.x ?? 0}px"
    style:top="{pos?.y ?? 0}px"
    {onkeydown}
  >
    {#each items as entry, i (i)}
      {#if isSeparator(entry)}
        <div class="separator" role="separator"></div>
      {:else if !isItem(entry)}
        <div class="heading" role="presentation">{entry.heading}</div>
      {:else}
        <button
          bind:this={buttons[i]}
          type="button"
          class="item"
          class:danger={entry.danger}
          class:active={active === i}
          role={entry.checked === undefined ? "menuitem" : "menuitemcheckbox"}
          aria-checked={entry.checked}
          aria-disabled={entry.disabled || undefined}
          aria-haspopup={entry.items?.length ? "menu" : undefined}
          aria-expanded={entry.items?.length ? submenu === i : undefined}
          tabindex={active === i ? 0 : -1}
          onclick={() => activate(i)}
          onpointermove={() => hover(i)}
        >
          {#if entry.checked !== undefined}
            <span class="check">{#if entry.checked}<Icon name="check" size="sm" />{/if}</span>
          {:else if entry.icon}
            <Icon name={entry.icon} size="sm" />
          {/if}
          <span class="text">{entry.label}</span>
          {#if entry.hint}<span class="hint">{entry.hint}</span>{/if}
          {#if entry.shortcut}<Kbd keys={entry.shortcut} size="sm" />{/if}
          {#if entry.items?.length}<Icon name="chevron-right" size="sm" class="sub" />{/if}
        </button>
        {#if entry.items?.length && submenu === i}
          <Self
            items={entry.items}
            anchor={buttons[i]}
            open={true}
            placement="right-start"
            label={entry.label}
            nested
            root={rootId}
            onselect={pick}
            onclose={() => closedSubmenu(i)}
          />
        {/if}
      {/if}
    {/each}
  </div>
{/if}

<style>
  .rt-menu {
    position: fixed;
    z-index: var(--rt-z-popover);
    min-width: 200px;
    max-width: 360px;
    max-height: min(480px, calc(100vh - 16px));
    overflow-y: auto;
    box-sizing: border-box;
    padding: var(--rt-space-2);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-3);
    color: var(--rt-fg);
    box-shadow: var(--rt-elev-2);
    font: var(--rt-font-body);
    visibility: hidden;
    outline: none;
  }
  .placed {
    visibility: visible;
    animation: menu-in var(--rt-dur-fast) var(--rt-ease-out);
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-2px) scale(0.98);
    }
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    width: 100%;
    height: var(--rt-control-md);
    padding: 0 var(--rt-space-4);
    border: 0;
    border-radius: var(--rt-radius-md);
    background: transparent;
    color: var(--rt-fg);
    text-align: left;
    cursor: pointer;
    outline: none;
  }
  .item :global(.rt-icon) {
    color: var(--rt-fg-muted);
  }
  .item.active {
    background: var(--rt-bg-hover);
  }
  .item.danger,
  .item.danger :global(.rt-icon) {
    color: var(--rt-danger);
  }
  .item.danger.active {
    background: var(--rt-danger-bg);
  }
  .item[aria-disabled="true"] {
    opacity: 0.45;
    cursor: default;
  }
  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    color: var(--rt-fg-subtle);
    font-size: var(--rt-text-small);
  }
  .check {
    display: inline-flex;
    width: var(--rt-icon-sm);
    color: var(--rt-accent);
  }
  .check :global(.rt-icon) {
    color: var(--rt-accent);
  }
  .item :global(.sub) {
    margin-right: calc(-1 * var(--rt-space-2));
  }
  .separator {
    height: 1px;
    margin: var(--rt-space-2) calc(-1 * var(--rt-space-2));
    background: var(--rt-border);
  }
  .heading {
    padding: var(--rt-space-3) var(--rt-space-4) var(--rt-space-2);
    color: var(--rt-fg-subtle);
    font: var(--rt-font-caption);
    letter-spacing: var(--rt-tracking-caps);
    text-transform: uppercase;
  }
</style>

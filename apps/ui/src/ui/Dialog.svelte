<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The modal dialog frame: backdrop, header (icon, title, description, close), scrolling body
     and an optional footer (the `footer` snippet, or `DialogFooter` at the end of the content,
     which is how dialogs shown by `AppState.showDialog` add theirs). Escape and the close
     button call `onclose`; focus moves in on open, stays inside and returns on close.
     Mount it while open (`{#if}`); the shell's DialogHost uses it for extension dialogs. -->
<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  import DialogFooter from "./DialogFooter.svelte";
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import type { IconName } from "./icons";

  let {
    title,
    description,
    icon,
    size = "md",
    closeOnBackdrop = true,
    closeLabel = "Close",
    testid,
    onclose,
    footer,
    children,
  }: {
    /** Header text and the dialog's accessible name. */
    title: string;
    description?: string;
    icon?: IconName | (string & {});
    /** sm 400 px, md 560 px, lg 760 px, xl 960 px, or a width in px; never wider than the
     *  window minus 32 px. */
    size?: "sm" | "md" | "lg" | "xl" | number;
    closeOnBackdrop?: boolean;
    closeLabel?: string;
    testid?: string;
    onclose: () => void;
    footer?: Snippet;
    children: Snippet;
  } = $props();

  const WIDTHS = { sm: 400, md: 560, lg: 760, xl: 960 };
  const width = $derived(typeof size === "number" ? size : WIDTHS[size]);
  const uid = $props.id();
  let panel = $state<HTMLElement>();
  let body = $state<HTMLElement>();

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), [contenteditable="true"]';

  const focusables = () =>
    [...(panel?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])].filter((el) => el.offsetParent !== null || el === document.activeElement);

  onMount(() => {
    const before = document.activeElement as HTMLElement | null;
    const target =
      panel?.querySelector<HTMLElement>("[autofocus], [data-autofocus]") ??
      body?.querySelector<HTMLElement>(FOCUSABLE) ??
      panel;
    target?.focus();
    return () => {
      if (before?.isConnected) before.focus();
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
      return;
    }
    if (e.key !== "Tab" || !panel) return;
    const list = focusables();
    if (list.length === 0) {
      e.preventDefault();
      panel.focus();
      return;
    }
    const first = list[0];
    const last = list[list.length - 1];
    const active = document.activeElement as HTMLElement | null;
    const inside = active && panel.contains(active);
    if (e.shiftKey && (active === first || !inside)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !inside)) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="rt-dialog-backdrop">
  <!-- The backdrop is a mouse convenience; keyboard users close with Escape or the button. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={() => closeOnBackdrop && onclose()}></div>
  <div
    bind:this={panel}
    class="rt-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="{uid}-title"
    aria-describedby={description ? `${uid}-desc` : undefined}
    tabindex="-1"
    data-testid={testid}
    style:width="min({width}px, calc(100vw - 32px))"
  >
    <header>
      {#if icon}<span class="icon"><Icon name={icon} /></span>{/if}
      <div class="titles">
        <h2 id="{uid}-title">{title}</h2>
        {#if description}<p id="{uid}-desc">{description}</p>{/if}
      </div>
      <IconButton icon="close" label={closeLabel} size="sm" onclick={onclose} tooltipPlacement="bottom" />
    </header>
    <div class="body" bind:this={body}>
      {@render children()}
      {#if footer}<DialogFooter>{@render footer()}</DialogFooter>{/if}
    </div>
  </div>
</div>

<style>
  .rt-dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--rt-z-dialog);
    display: grid;
    place-items: center;
    padding: var(--rt-space-6);
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--rt-bg-overlay);
    animation: fade var(--rt-dur-normal) var(--rt-ease-out);
  }
  .rt-dialog {
    --dlg-px: var(--rt-space-7);
    --dlg-py: var(--rt-space-6);
    position: relative;
    display: flex;
    flex-direction: column;
    max-height: calc(100vh - 64px);
    box-sizing: border-box;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-3);
    color: var(--rt-fg);
    box-shadow: var(--rt-elev-3);
    font: var(--rt-font-body);
    outline: none;
    animation: rise var(--rt-dur-normal) var(--rt-ease);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: var(--rt-space-4);
    padding: var(--dlg-py) var(--rt-space-5) 0 var(--dlg-px);
  }
  .icon {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 28px;
    height: 28px;
    margin-top: -2px;
    border-radius: var(--rt-radius-md);
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .titles {
    flex: 1;
    min-width: 0;
    padding-top: 2px;
  }
  h2 {
    margin: 0;
    font: var(--rt-font-title);
    font-size: 15px;
    letter-spacing: var(--rt-tracking-tight);
  }
  p {
    margin: var(--rt-space-1) 0 0;
    color: var(--rt-fg-muted);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--rt-space-5) var(--dlg-px) var(--dlg-py);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.985);
    }
  }
</style>

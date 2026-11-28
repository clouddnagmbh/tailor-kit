<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The consent popover (wave 8): what ONE call sends, small and at the place of the task, instead of
     a dialog. It shows destination, model, size and the number of masked secrets, a link to the raw
     call ("Show what is sent") and one button, "Send". Enter sends (the button has the focus),
     Escape cancels and nothing is sent. The popover is shown for every call; whoever hosts it decides
     what "Send" starts. Add-ons use the same component, so every AI call asks the same way.

     Placement: `anchor` (an element or a rectangle) puts it below (or above, if there is no room)
     that place, `fixed`; without an anchor it sits at the top centre of the window. -->
<script lang="ts">
  import { Button } from "@tailor-kit/ui";
  import { onMount } from "svelte";

  import { consentLine } from "./consent";
  import type { AiPreview } from "./types";

  interface Props {
    preview: AiPreview;
    anchor?: HTMLElement | DOMRect | null;
    /** What the call is for, shown as the heading ("Suggest a commit message"). */
    title?: string;
    onsend: () => void;
    oncancel: () => void;
    /** Opens the exact call; the host decides how (a dialog). */
    onshow?: () => void;
  }

  let { preview, anchor = null, title, onsend, oncancel, onshow }: Props = $props();

  let box: HTMLDivElement | undefined = $state();
  let style = $state("top: 48px; left: 50%; transform: translateX(-50%);");
  let send: HTMLButtonElement | undefined = $state();

  const line = $derived(consentLine(preview));
  const WIDTH = 380;

  function place() {
    if (!anchor || typeof window === "undefined") return;
    const r = anchor instanceof HTMLElement ? anchor.getBoundingClientRect() : anchor;
    const height = box?.offsetHeight ?? 120;
    const left = Math.max(8, Math.min(window.innerWidth - WIDTH - 8, r.left));
    const below = r.bottom + 6;
    const top = below + height > window.innerHeight - 8 ? Math.max(8, r.top - height - 6) : below;
    style = `top: ${top}px; left: ${left}px;`;
  }

  onMount(() => {
    place();
    send?.focus();
    const away = (e: PointerEvent) => {
      if (box && !box.contains(e.target as Node)) oncancel();
    };
    // After this click has finished, so the press that opened the popover does not close it.
    const t = setTimeout(() => document.addEventListener("pointerdown", away, true), 0);
    return () => {
      clearTimeout(t);
      document.removeEventListener("pointerdown", away, true);
    };
  });

  function key(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      oncancel();
    }
  }
</script>

<svelte:window onresize={place} />

<div
  bind:this={box}
  class="consent"
  role="dialog"
  tabindex="-1"
  aria-label={title ? `Send to the AI provider: ${title}` : "Send to the AI provider"}
  data-testid="ai-consent"
  {style}
  onkeydown={key}
>
  {#if title}<p class="title">{title}</p>{/if}
  <p class="line" data-testid="ai-consent-line">{line}</p>
  <div class="actions">
    {#if onshow}
      <Button variant="ghost" size="sm" data-testid="ai-consent-show" onclick={onshow}>Show what is sent</Button>
    {/if}
    <span class="spacer"></span>
    <Button size="sm" data-testid="ai-consent-cancel" onclick={oncancel}>Cancel</Button>
    <button bind:this={send} type="button" class="send" data-testid="ai-consent-send" onclick={onsend}>Send <kbd>⏎</kbd></button>
  </div>
</div>

<style>
  .consent {
    position: fixed;
    z-index: 60;
    box-sizing: border-box;
    width: 380px;
    max-width: calc(100vw - 16px);
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
    padding: var(--rt-space-4);
    color: var(--rt-fg);
    background: var(--rt-surface-1, var(--rt-surface-2));
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-md);
    box-shadow: var(--rt-shadow-3, 0 8px 24px rgb(0 0 0 / 0.25));
  }
  p {
    margin: 0;
  }
  .title {
    font: var(--rt-font-small);
    font-weight: 600;
  }
  .line {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
  }
  .spacer {
    flex: 1;
  }
  .send {
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-2);
    padding: var(--rt-space-2) var(--rt-space-4);
    color: var(--rt-accent-fg, #fff);
    background: var(--rt-accent, #2b6cb0);
    border: 1px solid transparent;
    border-radius: var(--rt-radius-sm);
    font: var(--rt-font-small);
    font-weight: 600;
    cursor: pointer;
  }
  .send:focus-visible {
    outline: 2px solid var(--rt-focus, currentColor);
    outline-offset: 2px;
  }
  kbd {
    font: inherit;
    opacity: 0.8;
  }
</style>

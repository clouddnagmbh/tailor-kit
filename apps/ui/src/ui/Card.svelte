<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A raised container: a header (icon, title, description, actions), content, a footer.
     `onclick` makes the whole card a choice (a tile in the "New…" dialog); such a card has no
     `actions` (no buttons inside a button). -->
<script lang="ts">
  import type { Snippet } from "svelte";

  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  let {
    title,
    description,
    icon,
    padding = "md",
    selected = false,
    onclick,
    actions,
    footer,
    children,
  }: {
    title?: string;
    description?: string;
    icon?: IconName | (string & {});
    padding?: "none" | "sm" | "md";
    selected?: boolean;
    onclick?: (e: MouseEvent) => void;
    actions?: Snippet;
    footer?: Snippet;
    children?: Snippet;
  } = $props();
</script>

{#snippet content()}
  {#if title || icon || actions}
    <header>
      {#if icon}<span class="icon"><Icon name={icon} /></span>{/if}
      {#if title || description}
        <div class="titles">
          {#if title}<h3>{title}</h3>{/if}
          {#if description}<p>{description}</p>{/if}
        </div>
      {/if}
      {#if actions}<div class="actions">{@render actions()}</div>{/if}
    </header>
  {/if}
  {#if children}<div class="body">{@render children()}</div>{/if}
  {#if footer}<footer>{@render footer()}</footer>{/if}
{/snippet}

{#if onclick}
  <!-- A div, not a <button>: headings and blocks are not allowed inside buttons. -->
  <div
    class="rt-card p-{padding} interactive"
    class:selected
    role="button"
    tabindex="0"
    aria-pressed={selected}
    {onclick}
    onkeydown={(e) => {
      if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
      e.preventDefault();
      e.currentTarget.click();
    }}
  >
    {@render content()}
  </div>
{:else}
  <section class="rt-card p-{padding}" class:selected>{@render content()}</section>
{/if}

<style>
  .rt-card {
    --pad: var(--rt-space-6);
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
    box-sizing: border-box;
    padding: var(--pad);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-2);
    color: var(--rt-fg);
    box-shadow: var(--rt-elev-1);
    font: var(--rt-font-body);
    text-align: left;
  }
  .p-sm {
    --pad: var(--rt-space-5);
    gap: var(--rt-space-4);
  }
  .p-none {
    --pad: 0;
    gap: 0;
    overflow: hidden;
  }
  .interactive {
    width: 100%;
    cursor: pointer;
    transition:
      border-color var(--rt-dur-fast) var(--rt-ease),
      box-shadow var(--rt-dur-fast) var(--rt-ease),
      transform var(--rt-dur-fast) var(--rt-ease);
  }
  .interactive:hover {
    border-color: var(--rt-border-strong);
    box-shadow: var(--rt-elev-2);
  }
  .interactive:active {
    transform: translateY(0.5px);
  }
  .interactive:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: var(--rt-focus-offset);
  }
  .selected {
    border-color: var(--rt-accent);
    box-shadow:
      0 0 0 1px var(--rt-accent),
      var(--rt-elev-1);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: var(--rt-space-4);
  }
  .icon {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--rt-radius-md);
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font: var(--rt-font-title);
    letter-spacing: var(--rt-tracking-tight);
  }
  p {
    margin: var(--rt-space-1) 0 0;
    color: var(--rt-fg-muted);
  }
  .actions {
    display: flex;
    flex: none;
    gap: var(--rt-space-2);
    margin: calc(-1 * var(--rt-space-2)) calc(-1 * var(--rt-space-2)) 0 0;
  }
  .p-none header {
    padding: var(--rt-space-5) var(--rt-space-6);
    border-bottom: 1px solid var(--rt-border);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--rt-space-4);
    margin: 0 calc(-1 * var(--pad)) calc(-1 * var(--pad));
    padding: var(--rt-space-4) var(--pad);
    border-top: 1px solid var(--rt-border);
    border-radius: 0 0 var(--rt-radius-lg) var(--rt-radius-lg);
    background: var(--rt-bg-panel);
  }
  .p-none footer {
    margin: 0;
    padding: var(--rt-space-4) var(--rt-space-6);
  }
</style>

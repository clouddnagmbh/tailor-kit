<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A note across a view: info, ok, warn or danger, with optional title, actions and a dismiss
     button. Danger is announced (role="alert"), the rest politely (role="status"). -->
<script lang="ts">
  import type { Snippet } from "svelte";

  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  let {
    tone = "info",
    title,
    icon,
    dismissible = false,
    dismissLabel = "Dismiss",
    ondismiss,
    actions,
    children,
  }: {
    tone?: "info" | "ok" | "warn" | "danger";
    title?: string;
    icon?: IconName | (string & {});
    dismissible?: boolean;
    dismissLabel?: string;
    ondismiss?: () => void;
    actions?: Snippet;
    children?: Snippet;
  } = $props();

  const ICON = { info: "info", ok: "success", warn: "warning", danger: "error" } as const;
  let hidden = $state(false);
</script>

{#if !hidden}
  <div class="rt-banner {tone}" role={tone === "danger" ? "alert" : "status"}>
    <Icon name={icon ?? ICON[tone]} class="lead" />
    <div class="content">
      {#if title}<div class="title">{title}</div>{/if}
      {#if children}<div class="text">{@render children()}</div>{/if}
    </div>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
    {#if dismissible}
      <button
        type="button"
        class="dismiss"
        aria-label={dismissLabel}
        onclick={() => {
          hidden = true;
          ondismiss?.();
        }}
      >
        <Icon name="close" size="sm" />
      </button>
    {/if}
  </div>
{/if}

<style>
  .rt-banner {
    display: flex;
    align-items: flex-start;
    gap: var(--rt-space-4);
    padding: var(--rt-space-4) var(--rt-space-5);
    border: 1px solid var(--b);
    border-radius: var(--rt-radius-lg);
    background: var(--bg);
    color: var(--rt-fg);
    font: var(--rt-font-body);
  }
  .info {
    --bg: var(--rt-info-bg);
    --fg: var(--rt-info-fg);
    --b: var(--rt-info-border);
  }
  .ok {
    --bg: var(--rt-ok-bg);
    --fg: var(--rt-ok-fg);
    --b: var(--rt-ok-border);
  }
  .warn {
    --bg: var(--rt-warn-bg);
    --fg: var(--rt-warn-fg);
    --b: var(--rt-warn-border);
  }
  .danger {
    --bg: var(--rt-danger-bg);
    --fg: var(--rt-danger-fg);
    --b: var(--rt-danger-border);
  }
  .rt-banner :global(.lead) {
    margin-top: 1px;
    color: var(--fg);
  }
  .content {
    flex: 1;
    min-width: 0;
  }
  .title {
    color: var(--fg);
    font-weight: var(--rt-weight-semibold);
  }
  .text {
    color: var(--rt-fg);
  }
  .title + .text {
    margin-top: var(--rt-space-1);
    color: var(--rt-fg-muted);
  }
  .actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--rt-space-3);
    margin: calc(-1 * var(--rt-space-1)) 0;
  }
  .dismiss {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: var(--rt-control-sm);
    height: var(--rt-control-sm);
    margin: calc(-1 * var(--rt-space-1)) calc(-1 * var(--rt-space-2)) 0 0;
    padding: 0;
    border: 0;
    border-radius: var(--rt-radius-sm);
    background: transparent;
    color: var(--rt-fg-muted);
    cursor: pointer;
  }
  .dismiss:hover {
    background: color-mix(in srgb, var(--fg) 12%, transparent);
    color: var(--rt-fg);
  }
  .dismiss:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- What a view shows when it has nothing yet: an icon, a title that says what is missing, one
     sentence on why or what to do, and at most two actions. Never a bare "No data". -->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";

  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  let {
    icon,
    title,
    text,
    size = "md",
    actions,
    children,
    class: className = "",
    ...rest
  }: Omit<HTMLAttributes<HTMLDivElement>, "class" | "title" | "children"> & {
    icon?: IconName | (string & {});
    title: string;
    text?: string;
    size?: "sm" | "md";
    actions?: Snippet;
    children?: Snippet;
    class?: string;
  } = $props();
</script>

<div {...rest} class="rt-empty {size} {className}">
  {#if icon}
    <div class="icon"><Icon name={icon} size="lg" /></div>
  {/if}
  <h3>{title}</h3>
  {#if text}<p>{text}</p>{/if}
  {#if children}{@render children()}{/if}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</div>

<style>
  .rt-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--rt-space-3);
    width: 100%;
    max-width: 380px;
    box-sizing: border-box;
    margin: 0 auto;
    padding: var(--rt-space-8) var(--rt-space-6);
    text-align: center;
    font: var(--rt-font-body);
  }
  .sm {
    padding: var(--rt-space-6) var(--rt-space-5);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    margin-bottom: var(--rt-space-3);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: linear-gradient(180deg, var(--rt-surface-2), var(--rt-bg-panel));
    color: var(--rt-fg-muted);
    box-shadow:
      var(--rt-elev-1),
      0 0 0 4px color-mix(in srgb, var(--rt-bg-panel) 70%, transparent);
  }
  .sm .icon {
    width: 36px;
    height: 36px;
    margin-bottom: var(--rt-space-2);
  }
  h3 {
    margin: 0;
    color: var(--rt-fg);
    font: var(--rt-font-title);
    letter-spacing: var(--rt-tracking-tight);
  }
  p {
    margin: 0;
    color: var(--rt-fg-muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--rt-space-4);
    margin-top: var(--rt-space-4);
  }
</style>

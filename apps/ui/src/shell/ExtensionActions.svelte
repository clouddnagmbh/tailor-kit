<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The actions extensions add to a place (wave 16, strand C): on a request, a response, a run or
     bisect, a range of two revisions. At most two `inline` actions are buttons; every other
     action is in the "More" menu. Renders nothing when there are none, so builds without
     extensions look as before. Errors of an action show in the app's error line. -->
<script lang="ts">
  import { getApp } from "../state/app.svelte";
  import { Button, Dropdown, IconButton, type MenuEntry } from "../ui";
  import type { ExtensionAction } from "../extensions";
  import type { ExtensionActionRef } from "./extension-actions";

  let {
    actions,
    label,
    size = "sm",
    variant = "secondary",
    collapse = false,
  }: {
    actions: ExtensionActionRef<ExtensionAction>[];
    /** Accessible name of the group, e.g. "Request actions". */
    label: string;
    size?: "sm" | "md";
    variant?: "secondary" | "ghost";
    /** In a narrow window the inline actions shrink to icon buttons (the label is the tooltip). */
    collapse?: boolean;
  } = $props();

  let width = $state(1280);
  const compact = $derived(collapse && width < 1100);

  const app = getApp();
  const inline = $derived(actions.filter((a) => a.action.inline).slice(0, 2));
  const menu = $derived(actions.filter((a) => !inline.includes(a)));

  function run(a: ExtensionActionRef<ExtensionAction>) {
    try {
      const r = a.action.run();
      if (r instanceof Promise) r.catch((e: unknown) => (app.error = e instanceof Error ? e.message : String(e)));
    } catch (e) {
      app.error = e instanceof Error ? e.message : String(e);
    }
  }

  const items = $derived<MenuEntry[]>(menu.map((a) => ({ id: a.key, label: a.action.label, icon: a.action.icon, run: () => run(a) })));
</script>

<svelte:window bind:innerWidth={width} />

{#if actions.length > 0}
  <div class="actions" role="group" aria-label={label} data-testid="extension-actions">
    {#each inline as a (a.key)}
      {#if compact}
        <IconButton icon={a.action.icon ?? "spark"} label={a.action.label} {size} data-testid="extension-action" data-action={a.key} onclick={() => run(a)} />
      {:else}
        <Button {size} {variant} icon={a.action.icon ?? "spark"} data-testid="extension-action" data-action={a.key} onclick={() => run(a)}>
          {a.action.label}
        </Button>
      {/if}
    {/each}
    {#if menu.length > 0}
      <Dropdown {items} menuLabel={label} placement="bottom-end">
        {#snippet trigger(props)}
          <IconButton {...props} icon="more" label="More {label.toLowerCase()}" {size} data-testid="extension-actions-menu" />
        {/snippet}
      </Dropdown>
    {/if}
  </div>
{/if}

<style>
  .actions {
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-3);
    flex-wrap: wrap;
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- Command palette (Mod+K): fuzzy filter over the shell's commands. Up/Down choose, Enter
     runs, Escape closes. Focus returns to where it was before the palette opened. -->
<script lang="ts">
  import { tick } from "svelte";

  import type { Command } from "./command";
  import { fuzzyFilter } from "./fuzzy";

  let {
    open = $bindable(false),
    commands,
    idPrefix,
  }: {
    open?: boolean;
    commands: Command[];
    /** Prefix of the element ids (`<prefix>-palette-list`, `<prefix>-cmd-<n>`), the product's short name. */
    idPrefix: string;
  } = $props();

  let query = $state("");
  let index = $state(0);
  let listEl = $state<HTMLElement>();
  let returnFocus: HTMLElement | null = null;

  const shown = $derived(fuzzyFilter(commands, query, (c) => c.label));
  const current = $derived(shown[Math.min(index, shown.length - 1)]);

  $effect.pre(() => {
    if (open) {
      returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      query = "";
      index = 0;
    }
  });

  async function close() {
    open = false;
    await tick();
    if (returnFocus?.isConnected) returnFocus.focus();
    returnFocus = null;
  }

  async function run(command: Command | undefined) {
    if (!command) return;
    await close();
    await command.run();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") index = Math.min(index + 1, shown.length - 1);
    else if (e.key === "ArrowUp") index = Math.max(index - 1, 0);
    else if (e.key === "Enter") run(current);
    else if (e.key === "Escape") close();
    else if (e.key === "Tab") {
      // Focus stays in the palette (modal).
    } else return;
    e.preventDefault();
    e.stopPropagation();
    tick().then(() => listEl?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" }));
  }

  const autofocus = (el: HTMLInputElement) => {
    el.focus();
  };
</script>

{#if open}
  <!-- Clicking the backdrop closes; the keyboard equivalent is Escape in the input. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={close}></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette" data-testid="palette">
    <input
      role="combobox"
      aria-expanded="true"
      aria-controls="{idPrefix}-palette-list"
      aria-autocomplete="list"
      aria-activedescendant={current ? `${idPrefix}-cmd-${shown.indexOf(current)}` : undefined}
      aria-label="Command"
      placeholder="Type a command"
      data-testid="palette-input"
      bind:value={query}
      oninput={() => (index = 0)}
      {onkeydown}
      {@attach autofocus}
    />
    <ul id="{idPrefix}-palette-list" role="listbox" aria-label="Commands" bind:this={listEl}>
      {#each shown as command, i (command.id)}
        <!-- Options are chosen with the arrow keys in the input (aria-activedescendant). -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li
          id="{idPrefix}-cmd-{i}"
          role="option"
          aria-selected={command === current}
          class:current={command === current}
          data-testid="palette-option"
          onclick={() => run(command)}
          onpointermove={() => (index = i)}
        >
          <span class="label">{command.label}</span>
          {#if command.checked}<span class="check" aria-label="current">✓</span>{/if}
          {#if command.shortcut}<kbd>{command.shortcut}</kbd>{/if}
        </li>
      {:else}
        <li class="none" role="presentation">No matching command</li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--rt-bg-overlay);
    z-index: 20;
  }
  .palette {
    position: fixed;
    z-index: 21;
    top: 12vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, calc(100vw - 32px));
    background: var(--rt-bg);
    border: 1px solid var(--rt-border);
    border-radius: calc(var(--rt-radius) * 2);
    box-shadow: var(--rt-shadow);
    overflow: hidden;
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: 12px 14px;
    border: 0;
    border-bottom: 1px solid var(--rt-border);
    background: transparent;
    color: var(--rt-fg);
    font: inherit;
    font-size: 14px;
    outline: none;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 4px;
    max-height: 50vh;
    overflow: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--rt-gap);
    padding: 6px 10px;
    border-radius: var(--rt-radius);
    cursor: pointer;
  }
  li.current {
    background: var(--rt-bg-selected);
  }
  .label {
    flex: 1;
  }
  .check {
    color: var(--rt-accent);
  }
  kbd {
    font: var(--rt-size-small) var(--rt-font-ui);
    color: var(--rt-fg-muted);
  }
  .none {
    color: var(--rt-fg-muted);
    cursor: default;
  }
</style>

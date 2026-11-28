<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- The sparkle button of an AI task at its place (wave 8): a small icon button with an accessible name
     and, while a call runs, a busy state that turns into "stop". -->
<script lang="ts">
  import { Icon } from "@tailor-kit/ui";

  interface Props {
    /** The accessible name and tooltip ("Suggest a commit message"). */
    label: string;
    busy?: boolean;
    disabled?: boolean;
    onclick: (anchor: HTMLElement) => void;
    "data-testid"?: string;
  }

  let { label, busy = false, disabled = false, onclick, "data-testid": testid }: Props = $props();
  let el: HTMLButtonElement | undefined = $state();
</script>

<button
  bind:this={el}
  type="button"
  class="inline-ai"
  class:busy
  {disabled}
  aria-label={busy ? `Stop: ${label}` : label}
  title={label}
  data-testid={testid ?? "ai-inline-button"}
  onclick={() => el && onclick(el)}
>
  <Icon name={busy ? "close" : "spark"} size="sm" />
</button>

<style>
  .inline-ai {
    display: inline-grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    color: var(--rt-fg-muted);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--rt-radius-sm);
    cursor: pointer;
  }
  .inline-ai:hover:not(:disabled) {
    color: var(--rt-fg);
    background: var(--rt-surface-2);
  }
  .inline-ai:focus-visible {
    outline: 2px solid var(--rt-focus, currentColor);
    outline-offset: 1px;
  }
  .inline-ai.busy {
    color: var(--rt-accent, currentColor);
  }
  .inline-ai:disabled {
    opacity: 0.5;
    cursor: default;
  }
  @media (prefers-reduced-motion: no-preference) {
    .inline-ai.busy {
      animation: pulse 1.2s ease-in-out infinite;
    }
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
</style>

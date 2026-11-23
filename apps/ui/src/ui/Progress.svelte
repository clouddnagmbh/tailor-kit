<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- A progress bar. With `value` it is determinate (`value` of `max`); without, it is
     indeterminate. `label` is required: it is the accessible name. -->
<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";

  let {
    value,
    max = 100,
    label,
    size = "md",
    tone = "accent",
    class: className = "",
    ...rest
  }: Omit<HTMLAttributes<HTMLDivElement>, "class" | "children"> & {
    /** Omit for an indeterminate bar. */
    value?: number;
    max?: number;
    label: string;
    size?: "sm" | "md";
    tone?: "accent" | "ok" | "warn" | "danger";
    class?: string;
  } = $props();

  const determinate = $derived(value !== undefined && Number.isFinite(value));
  const ratio = $derived(determinate && max > 0 ? Math.min(1, Math.max(0, (value as number) / max)) : 0);
</script>

<div
  {...rest}
  class="rt-progress {size} t-{tone} {className}"
  class:indeterminate={!determinate}
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={max}
  aria-valuenow={determinate ? (value as number) : undefined}
>
  <div class="bar" style={determinate ? `width: ${ratio * 100}%` : undefined}></div>
</div>

<style>
  .rt-progress {
    --h: 6px;
    --c: var(--rt-accent);
    position: relative;
    overflow: hidden;
    box-sizing: border-box;
    width: 100%;
    height: var(--h);
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-sunken);
    box-shadow: inset 0 0 0 1px var(--rt-border);
  }
  .sm {
    --h: 4px;
  }
  .t-ok {
    --c: var(--rt-ok);
  }
  .t-warn {
    --c: var(--rt-warn);
  }
  .t-danger {
    --c: var(--rt-danger);
  }
  .bar {
    height: 100%;
    border-radius: inherit;
    background: var(--c);
    transition: width var(--rt-dur-normal) var(--rt-ease);
  }
  .indeterminate .bar {
    position: absolute;
    width: 40%;
    animation: slide 1.4s var(--rt-ease) infinite;
  }
  @keyframes slide {
    from {
      left: -40%;
    }
    to {
      left: 100%;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar {
      transition: none;
    }
    .indeterminate .bar {
      width: 100%;
      left: 0;
      animation: pulse 2.4s ease-in-out infinite;
    }
    @keyframes pulse {
      50% {
        opacity: 0.35;
      }
    }
  }
</style>

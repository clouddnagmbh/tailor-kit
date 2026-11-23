<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- An indeterminate progress ring in the current text colour. -->
<script lang="ts">
  let { size = "md", label = "Loading" }: { size?: "sm" | "md" | "lg"; label?: string } = $props();
</script>

<span class="rt-spinner {size}" role="status" aria-label={label}>
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <circle class="track" cx="8" cy="8" r="6.25" />
    <circle class="arc" cx="8" cy="8" r="6.25" />
  </svg>
</span>

<style>
  .rt-spinner {
    display: inline-flex;
    flex: none;
    width: var(--rt-icon-md);
    height: var(--rt-icon-md);
  }
  .sm {
    width: var(--rt-icon-sm);
    height: var(--rt-icon-sm);
  }
  .lg {
    width: var(--rt-icon-lg);
    height: var(--rt-icon-lg);
  }
  svg {
    width: 100%;
    height: 100%;
    animation: spin 0.8s linear infinite;
  }
  circle {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.75;
  }
  .track {
    opacity: 0.2;
  }
  .arc {
    stroke-linecap: round;
    stroke-dasharray: 39.3;
    stroke-dashoffset: 28;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    svg {
      animation-duration: 2.4s;
    }
  }
</style>

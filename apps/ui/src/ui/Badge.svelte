<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Small, non-interactive labels.
     - `kind="count"`: a number (99+ above `max`);
     - `kind="status"`: a Git file status letter A, M, D, R, U (conflict) or ? (untracked);
     - `kind="method"`: an HTTP method (GET, POST, …), also GRPC, WS, GQL;
     - `kind="label"` (default): a word such as "Pro", "New", "Beta", in a `tone`. -->
<script lang="ts">
  type Tone = "neutral" | "accent" | "info" | "ok" | "warn" | "danger";

  let {
    kind = "label",
    value,
    tone = "neutral",
    max = 99,
    solid = false,
    title,
  }: {
    kind?: "count" | "status" | "method" | "label";
    value: string | number;
    tone?: Tone;
    max?: number;
    /** Filled background (counts on a tab, "Pro" on a tile). */
    solid?: boolean;
    title?: string;
  } = $props();

  const STATUS: Record<string, [Tone, string]> = {
    A: ["ok", "Added"],
    M: ["warn", "Modified"],
    D: ["danger", "Deleted"],
    R: ["info", "Renamed"],
    U: ["danger", "Conflict"],
    "?": ["neutral", "Untracked"],
  };
  const METHODS = ["get", "post", "put", "patch", "delete"];

  const text = $derived(kind === "count" && typeof value === "number" && value > max ? `${max}+` : String(value));
  const status = $derived(kind === "status" ? (STATUS[String(value).toUpperCase()] ?? ["neutral", String(value)]) : null);
  const effectiveTone = $derived(status ? status[0] : tone);
  const method = $derived(
    kind === "method" ? (METHODS.includes(String(value).toLowerCase()) ? String(value).toLowerCase() : "other") : null,
  );
</script>

{#if method}
  <span class="rt-badge method m-{method}" title={title ?? text}>{text.toUpperCase()}</span>
{:else}
  <span
    class="rt-badge {kind} t-{effectiveTone}"
    class:solid
    title={title ?? status?.[1]}
    aria-label={status ? status[1] : undefined}>{text}</span
  >
{/if}

<style>
  .rt-badge {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    height: 18px;
    padding: 0 var(--rt-space-3);
    border: 1px solid transparent;
    border-radius: var(--rt-radius-sm);
    font: var(--rt-weight-semibold) var(--rt-text-caption) / 1 var(--rt-font-ui);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .count {
    min-width: 18px;
    padding: 0 var(--rt-space-2);
    border-radius: var(--rt-radius-pill);
  }
  .status {
    width: 18px;
    padding: 0;
    font-family: var(--rt-font-mono);
  }
  .t-neutral {
    background: var(--rt-surface-sunken);
    border-color: var(--rt-border);
    color: var(--rt-fg-muted);
  }
  .t-accent {
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .t-info {
    background: var(--rt-info-bg);
    border-color: var(--rt-info-border);
    color: var(--rt-info-fg);
  }
  .t-ok {
    background: var(--rt-ok-bg);
    border-color: var(--rt-ok-border);
    color: var(--rt-ok-fg);
  }
  .t-warn {
    background: var(--rt-warn-bg);
    border-color: var(--rt-warn-border);
    color: var(--rt-warn-fg);
  }
  .t-danger {
    background: var(--rt-danger-bg);
    border-color: var(--rt-danger-border);
    color: var(--rt-danger-fg);
  }
  .solid.t-neutral {
    background: var(--rt-fg-muted);
    border-color: transparent;
    color: var(--rt-bg);
  }
  .solid.t-accent,
  .solid.t-info {
    background: var(--rt-accent-solid);
    border-color: transparent;
    color: var(--rt-accent-solid-fg);
  }
  .solid.t-ok {
    background: var(--rt-ok);
    border-color: transparent;
    color: var(--rt-bg);
  }
  .solid.t-warn {
    background: var(--rt-warn);
    border-color: transparent;
    color: var(--rt-bg);
  }
  .solid.t-danger {
    background: var(--rt-danger-solid);
    border-color: transparent;
    color: var(--rt-fg-on-solid);
  }
  .method {
    --m: var(--rt-method-other);
    min-width: 40px;
    padding: 0 var(--rt-space-2);
    color: var(--m);
    background: color-mix(in srgb, var(--m) 12%, transparent);
    font: var(--rt-weight-bold) 10px / 1 var(--rt-font-mono);
    letter-spacing: 0.02em;
  }
  .m-get {
    --m: var(--rt-method-get);
  }
  .m-post {
    --m: var(--rt-method-post);
  }
  .m-put {
    --m: var(--rt-method-put);
  }
  .m-patch {
    --m: var(--rt-method-patch);
  }
  .m-delete {
    --m: var(--rt-method-delete);
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Drag handle between two panes. Reports the pointer offset since the drag started; arrow
     keys step by `step` pixels. The parent owns the size. -->
<script lang="ts">
  let {
    label,
    orientation,
    value,
    min,
    max,
    onresize,
    testid,
  }: {
    label: string;
    /** "vertical": a vertical bar that resizes left/right. */
    orientation: "vertical" | "horizontal";
    /** Current size in pixels (for ARIA and as the start of a drag). */
    value: number;
    min: number;
    max: number;
    onresize: (size: number) => void;
    testid?: string;
  } = $props();

  let start: { pos: number; value: number } | null = null;
  const pos = (e: PointerEvent) => (orientation === "vertical" ? e.clientX : e.clientY);
  const clamp = (v: number) => Math.round(Math.min(max, Math.max(min, v)));

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    start = { pos: pos(e), value };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function onpointermove(e: PointerEvent) {
    if (start) onresize(clamp(start.value + pos(e) - start.pos));
  }
  function onpointerup() {
    start = null;
  }
  function onkeydown(e: KeyboardEvent) {
    const step = e.shiftKey ? 64 : 16;
    const less = orientation === "vertical" ? "ArrowLeft" : "ArrowUp";
    const more = orientation === "vertical" ? "ArrowRight" : "ArrowDown";
    if (e.key === less) onresize(clamp(value - step));
    else if (e.key === more) onresize(clamp(value + step));
    else if (e.key === "Home") onresize(min);
    else if (e.key === "End") onresize(max);
    else return;
    e.preventDefault();
  }
</script>

<!-- A focusable separator is an interactive widget (WAI-ARIA window splitter pattern). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="resizer {orientation}"
  role="separator"
  aria-label={label}
  aria-orientation={orientation}
  aria-valuenow={Math.round(value)}
  aria-valuemin={Math.round(min)}
  aria-valuemax={Math.round(max)}
  tabindex="0"
  data-testid={testid}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={onpointerup}
  {onkeydown}
></div>

<style>
  .resizer {
    flex: none;
    position: relative;
    background: var(--rt-border);
    touch-action: none;
  }
  .resizer::after {
    content: "";
    position: absolute;
    inset: -3px;
  }
  .vertical {
    width: 1px;
    cursor: col-resize;
  }
  .horizontal {
    height: 1px;
    cursor: row-resize;
  }
  .resizer:hover,
  .resizer:focus-visible {
    background: var(--rt-accent);
    outline: none;
  }
  .vertical:hover,
  .vertical:focus-visible {
    box-shadow: 0 0 0 1px var(--rt-accent);
  }
  .horizontal:hover,
  .horizontal:focus-visible {
    box-shadow: 0 0 0 1px var(--rt-accent);
  }
</style>

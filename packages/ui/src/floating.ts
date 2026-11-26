// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Floating layers (menus, tooltips) live in a portal at the end of <body>, so `overflow: hidden`
// of a panel never clips them. They take the theme of their anchor (`data-theme` of the closest
// themed ancestor), so a scoped theme carries over.

import type { Rect } from "./position";

/** The `data-theme` an element sees, if a scoped one is set. */
export function themeOf(el: Element | null | undefined): string | undefined {
  return el?.closest("[data-theme]")?.getAttribute("data-theme") ?? undefined;
}

/** Svelte action: moves the node to the end of <body> (and copies the anchor's theme). */
export function portal(node: HTMLElement, anchor?: Element | null) {
  const theme = themeOf(anchor);
  if (theme) node.setAttribute("data-theme", theme);
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}

export function rectOf(anchor: Element | { x: number; y: number }): Rect {
  if ("getBoundingClientRect" in anchor) {
    const r = anchor.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }
  return { x: anchor.x, y: anchor.y, width: 0, height: 0 };
}

export const viewport = () => ({ width: window.innerWidth, height: window.innerHeight });

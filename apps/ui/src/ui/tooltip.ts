// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Tooltips: `use:tooltip={"Text"}` or `use:tooltip={{ text, shortcut, placement }}` on any
// element. Shown after a delay on hover and on keyboard focus; once one tooltip was visible,
// the next shows at once for a short while (moving along a toolbar). Hidden on leave, blur,
// Escape and press. Rendered in a portal, so it is never clipped.
//
// `TooltipTimer` is the timing model without a DOM (tested in tooltip.test.ts).

import "./floating.css";
import { portal, rectOf, viewport } from "./floating";
import { formatKeys } from "./keys";
import { place, type Placement } from "./position";
import { isMacPlatform } from "../shell/shortcuts";

export interface TooltipOptions {
  text: string;
  /** A shortcut in `Kbd` notation, shown after the text. */
  shortcut?: string;
  placement?: Placement;
  /** Delay before showing in ms (default 500). */
  delay?: number;
  /** Also link the tooltip as the element's description (`aria-describedby`). Off when the
   *  text is already the element's accessible name (IconButton). Default true. */
  describe?: boolean;
  disabled?: boolean;
}

export interface Clock {
  now(): number;
  set(fn: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

const realClock: Clock = {
  now: () => Date.now(),
  set: (fn, ms) => setTimeout(fn, ms),
  clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

/** Shared across all tooltips: when the last one was hidden (warm window). */
const shared = { lastHidden: -Infinity };

export const DEFAULT_DELAY = 500;
/** After a tooltip hides, the next one within this window shows without delay. */
export const WARM_WINDOW = 400;

export class TooltipTimer {
  visible = false;
  private pending: unknown = null;

  constructor(
    private readonly onchange: (visible: boolean) => void,
    private readonly delay = DEFAULT_DELAY,
    private readonly clock: Clock = realClock,
    private readonly warm: { lastHidden: number } = shared,
  ) {}

  /** Pointer entered or keyboard focus arrived. */
  enter(): void {
    if (this.visible || this.pending !== null) return;
    if (this.clock.now() - this.warm.lastHidden < WARM_WINDOW) return this.show();
    this.pending = this.clock.set(() => {
      this.pending = null;
      this.show();
    }, this.delay);
  }

  /** Pointer left, focus left, Escape, a press or the element went away. */
  leave(): void {
    if (this.pending !== null) {
      this.clock.clear(this.pending);
      this.pending = null;
    }
    if (!this.visible) return;
    this.visible = false;
    this.warm.lastHidden = this.clock.now();
    this.onchange(false);
  }

  private show(): void {
    this.visible = true;
    this.onchange(true);
  }
}

let seq = 0;

type Param = string | TooltipOptions;
const normalise = (p: Param): TooltipOptions => (typeof p === "string" ? { text: p } : p);

/** Svelte action. */
export function tooltip(node: HTMLElement, param: Param) {
  let options = normalise(param);
  const id = `rt-tooltip-${++seq}`;
  let tip: HTMLElement | null = null;
  let portalHandle: { destroy(): void } | null = null;

  const render = () => {
    if (!tip) return;
    tip.textContent = options.text;
    if (options.shortcut) {
      const keys = document.createElement("span");
      keys.className = "rt-tooltip-keys";
      for (const k of formatKeys(options.shortcut, isMacPlatform())) {
        const kbd = document.createElement("kbd");
        kbd.textContent = k;
        keys.appendChild(kbd);
      }
      tip.appendChild(keys);
    }
    const r = tip.getBoundingClientRect();
    const at = place(rectOf(node), { width: r.width, height: r.height }, viewport(), options.placement ?? "top", 6);
    tip.style.left = `${at.x}px`;
    tip.style.top = `${at.y}px`;
    tip.dataset.side = at.placement.split("-")[0];
  };

  const timer = new TooltipTimer((visible) => {
    if (visible) {
      tip = document.createElement("div");
      tip.id = id;
      tip.className = "rt-tooltip";
      tip.setAttribute("role", "tooltip");
      portalHandle = portal(tip, node);
      render();
      if (options.describe !== false) node.setAttribute("aria-describedby", id);
    } else {
      portalHandle?.destroy();
      portalHandle = null;
      tip = null;
      if (node.getAttribute("aria-describedby") === id) node.removeAttribute("aria-describedby");
    }
  }, options.delay);

  const enter = () => {
    if (!options.disabled && options.text) timer.enter();
  };
  const focus = () => {
    // Keyboard focus only: a click focuses too, and must not pop a tooltip up.
    if (node.matches(":focus-visible")) enter();
  };
  const leave = () => timer.leave();
  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape" && timer.visible) timer.leave();
  };

  node.addEventListener("pointerenter", enter);
  node.addEventListener("pointerleave", leave);
  node.addEventListener("pointerdown", leave);
  node.addEventListener("focusin", focus);
  node.addEventListener("focusout", leave);
  node.addEventListener("keydown", key);

  return {
    update(next: Param) {
      options = normalise(next);
      if (options.disabled || !options.text) timer.leave();
      else render();
    },
    destroy() {
      timer.leave();
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("pointerleave", leave);
      node.removeEventListener("pointerdown", leave);
      node.removeEventListener("focusin", focus);
      node.removeEventListener("focusout", leave);
      node.removeEventListener("keydown", key);
    },
  };
}

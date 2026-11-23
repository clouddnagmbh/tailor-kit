// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { place } from "./position";
import { TooltipTimer, WARM_WINDOW, type Clock } from "./tooltip";

/** A clock the test advances by hand. */
function fakeClock() {
  let t = 1_000;
  let timers: { at: number; fn: () => void; id: number }[] = [];
  let next = 0;
  const clock: Clock = {
    now: () => t,
    set: (fn, ms) => {
      const id = ++next;
      timers.push({ at: t + ms, fn, id });
      return id;
    },
    clear: (id) => {
      timers = timers.filter((x) => x.id !== id);
    },
  };
  const advance = (ms: number) => {
    t += ms;
    const due = timers.filter((x) => x.at <= t);
    timers = timers.filter((x) => x.at > t);
    due.forEach((x) => x.fn());
  };
  return { clock, advance };
}

function timer(delay = 500) {
  const { clock, advance } = fakeClock();
  const events: boolean[] = [];
  const warm = { lastHidden: -Infinity };
  const t = new TooltipTimer((v) => events.push(v), delay, clock, warm);
  return { t, events, advance, clock, warm };
}

describe("tooltip timing", () => {
  it("shows after the delay on hover or keyboard focus", () => {
    const { t, events, advance } = timer();
    t.enter();
    advance(499);
    expect(events).toEqual([]);
    advance(1);
    expect(events).toEqual([true]);
    expect(t.visible).toBe(true);
  });

  it("never shows when the pointer leaves before the delay", () => {
    const { t, events, advance } = timer();
    t.enter();
    advance(300);
    t.leave();
    advance(1_000);
    expect(events).toEqual([]);
  });

  it("hides at once on Escape, blur or press (leave)", () => {
    const { t, events, advance } = timer();
    t.enter();
    advance(500);
    t.leave();
    expect(events).toEqual([true, false]);
    expect(t.visible).toBe(false);
  });

  it("shows the next tooltip without delay within the warm window", () => {
    const { clock, advance } = fakeClock();
    const warm = { lastHidden: -Infinity };
    const a: boolean[] = [];
    const b: boolean[] = [];
    const first = new TooltipTimer((v) => a.push(v), 500, clock, warm);
    const second = new TooltipTimer((v) => b.push(v), 500, clock, warm);
    first.enter();
    advance(500);
    first.leave();
    advance(WARM_WINDOW - 50);
    second.enter();
    expect(b).toEqual([true]);
    second.leave();
    advance(WARM_WINDOW + 1);
    first.enter();
    expect(a).toEqual([true, false]); // cold again: waits for the delay
  });

  it("does not stack timers on repeated enter", () => {
    const { t, events, advance } = timer();
    t.enter();
    t.enter();
    advance(500);
    expect(events).toEqual([true]);
  });
});

describe("placement", () => {
  const viewport = { width: 1000, height: 800 };
  const size = { width: 100, height: 30 };

  it("centres above the anchor by default", () => {
    expect(place({ x: 400, y: 400, width: 40, height: 20 }, size, viewport, "top", 6)).toEqual({ x: 370, y: 364, placement: "top" });
  });

  it("flips to the other side when there is no room", () => {
    expect(place({ x: 400, y: 10, width: 40, height: 20 }, size, viewport, "top", 6).placement).toBe("bottom");
    expect(place({ x: 400, y: 780, width: 40, height: 16 }, size, viewport, "bottom-start", 4).placement).toBe("top-start");
    expect(place({ x: 950, y: 400, width: 40, height: 20 }, size, viewport, "right-start", 4).placement).toBe("left-start");
  });

  it("stays inside the viewport", () => {
    const at = place({ x: 990, y: 400, width: 8, height: 20 }, size, viewport, "bottom-start", 4);
    expect(at.x).toBe(1000 - 100 - 8);
    const left = place({ x: 0, y: 400, width: 8, height: 20 }, size, viewport, "top", 6);
    expect(left.x).toBe(8);
  });

  it("aligns start and end", () => {
    const anchor = { x: 200, y: 200, width: 80, height: 24 };
    expect(place(anchor, size, viewport, "bottom-start", 4).x).toBe(200);
    expect(place(anchor, size, viewport, "bottom-end", 4).x).toBe(180);
  });
});

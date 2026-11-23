// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Placement of floating layers (menus, tooltips) next to an anchor: flips to the other side when
// there is no room and stays inside the viewport. Pure, so it is tested without a DOM.

export type Side = "top" | "bottom" | "left" | "right";
export type Placement = Side | `${Side}-start` | `${Side}-end`;

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Placed {
  x: number;
  y: number;
  /** The placement used after flipping. */
  placement: Placement;
}

const OPPOSITE: Record<Side, Side> = { top: "bottom", bottom: "top", left: "right", right: "left" };

function split(placement: Placement): [Side, "start" | "center" | "end"] {
  const [side, align] = placement.split("-") as [Side, "start" | "end" | undefined];
  return [side, align ?? "center"];
}

/** Room between the anchor and the viewport edge on `side`. */
function room(anchor: Rect, viewport: Size, side: Side): number {
  switch (side) {
    case "top":
      return anchor.y;
    case "bottom":
      return viewport.height - (anchor.y + anchor.height);
    case "left":
      return anchor.x;
    case "right":
      return viewport.width - (anchor.x + anchor.width);
  }
}

const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(v, Math.max(min, max)));

/**
 * Where to put a layer of `size` at `placement` of `anchor`, `offset` px away from it and at
 * least `margin` px inside the viewport.
 */
export function place(anchor: Rect, size: Size, viewport: Size, placement: Placement, offset = 6, margin = 8): Placed {
  let [side, align] = split(placement);
  const need = (side === "top" || side === "bottom" ? size.height : size.width) + offset + margin;
  if (room(anchor, viewport, side) < need && room(anchor, viewport, OPPOSITE[side]) > room(anchor, viewport, side)) {
    side = OPPOSITE[side];
  }
  let x: number;
  let y: number;
  if (side === "top" || side === "bottom") {
    y = side === "top" ? anchor.y - offset - size.height : anchor.y + anchor.height + offset;
    x =
      align === "start"
        ? anchor.x
        : align === "end"
          ? anchor.x + anchor.width - size.width
          : anchor.x + (anchor.width - size.width) / 2;
  } else {
    x = side === "left" ? anchor.x - offset - size.width : anchor.x + anchor.width + offset;
    y =
      align === "start"
        ? anchor.y
        : align === "end"
          ? anchor.y + anchor.height - size.height
          : anchor.y + (anchor.height - size.height) / 2;
  }
  return {
    x: Math.round(clamp(x, margin, viewport.width - size.width - margin)),
    y: Math.round(clamp(y, margin, viewport.height - size.height - margin)),
    placement: (align === "center" ? side : `${side}-${align}`) as Placement,
  };
}

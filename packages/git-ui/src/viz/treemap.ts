// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// A squarified treemap (wave 8, strand E; V13): areas proportional to sizes, as square as the
// numbers allow (Bruls, Huizing, van Wijk). Pure; the unit rectangle is `width × height` and the
// component maps it to percentages.

export interface TreemapItem {
  id: string;
  size: number;
}

export interface TreemapRect {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

function worst(row: readonly number[], side: number): number {
  const sum = row.reduce((a, b) => a + b, 0);
  const max = Math.max(...row);
  const min = Math.min(...row);
  return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
}

/** Rectangles for the items (zero and negative sizes are left out), larger items first. */
export function squarify(items: readonly TreemapItem[], width: number, height: number): TreemapRect[] {
  const list = items.filter((i) => i.size > 0).sort((a, b) => b.size - a.size || (a.id < b.id ? -1 : 1));
  const total = list.reduce((a, i) => a + i.size, 0);
  if (list.length === 0 || total <= 0 || width <= 0 || height <= 0) return [];
  const scale = (width * height) / total;
  const areas = list.map((i) => i.size * scale);
  const out: TreemapRect[] = [];
  let x = 0;
  let y = 0;
  let w = width;
  let h = height;
  let start = 0;
  while (start < list.length) {
    const side = Math.min(w, h);
    let end = start + 1;
    while (end < list.length && worst(areas.slice(start, end + 1), side) <= worst(areas.slice(start, end), side)) end++;
    const row = areas.slice(start, end);
    const sum = row.reduce((a, b) => a + b, 0);
    if (w >= h) {
      // A column on the left, as tall as the free space.
      const cw = sum / h;
      let cy = y;
      row.forEach((a, k) => {
        const ch = a / cw;
        out.push({ id: list[start + k]!.id, x, y: cy, w: cw, h: ch });
        cy += ch;
      });
      x += cw;
      w -= cw;
    } else {
      const rh = sum / w;
      let cx = x;
      row.forEach((a, k) => {
        const cwid = a / rh;
        out.push({ id: list[start + k]!.id, x: cx, y, w: cwid, h: rh });
        cx += cwid;
      });
      y += rh;
      h -= rh;
    }
    start = end;
  }
  return out;
}

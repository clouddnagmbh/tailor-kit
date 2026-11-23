// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: the lanes of the commit graph, computed once here for every shell (the
// shells only list commits in topological order, newest first). Pure and incremental: pages of
// the graph are added in order and continue exactly where the previous page ended.
//
// Model: a lane is a column that waits for one commit (the parent of a commit above it). A
// commit takes the leftmost lane that waits for it, or the first free one (a branch tip); every
// other lane waiting for it ends there (a fork, seen from above). Its first parent continues in
// its lane, every other parent (a merge) curves into the lane that already waits for that
// parent, or opens the first free lane. Lanes are never shifted, so a line stays in its column;
// freed lanes are reused. Each row describes only its own height, so a virtualised list draws
// any row on its own:
//
//   top half      `through` lines and `up` curves (from a lane into the commit)
//   ─ ● ─         the commit, in `lane`
//   bottom half   `through` lines and `down` curves (from the commit to a parent's lane)

export interface LaneCommit {
  oid: string;
  parents: string[];
}

export interface LaneRow {
  /** Column of the commit. */
  lane: number;
  /** Colour index of the commit's line (`--rt-lane-${color % 8 + 1}`). */
  color: number;
  /** A merge commit (two or more parents). */
  merge: boolean;
  /** Lines of other commits that pass the row from top to bottom in their column. */
  through: { lane: number; color: number }[];
  /** Lines from the top of the row into the commit (its own lane first if it continues). */
  up: { from: number; color: number }[];
  /** Lines from the commit to the bottom of the row, one per parent (first parent first). */
  down: { to: number; color: number }[];
  /** Columns used in this row (the highest one plus one). */
  width: number;
}

/** Colours cycle through the lane tokens. */
export const LANE_COLORS = 8;

export class LaneLayout {
  /** Per column: the commit it waits for, `null` when free. */
  private lanes: (string | null)[] = [];
  private colors: number[] = [];
  private nextColor = 0;
  /** The widest row so far. */
  maxWidth = 0;

  private freeLane(skip: number): number {
    for (let i = 0; i < this.lanes.length; i++) if (i !== skip && this.lanes[i] === null) return i;
    return Math.max(this.lanes.length, skip + 1);
  }

  private open(lane: number, oid: string): number {
    while (this.lanes.length <= lane) {
      this.lanes.push(null);
      this.colors.push(0);
    }
    this.lanes[lane] = oid;
    const color = this.nextColor++ % LANE_COLORS;
    this.colors[lane] = color;
    return color;
  }

  /** Lays out the next commits (continuing from the previous call). */
  add(commits: readonly LaneCommit[]): LaneRow[] {
    const rows: LaneRow[] = new Array(commits.length);
    for (let n = 0; n < commits.length; n++) {
      const { oid, parents } = commits[n];
      const lanes = this.lanes;
      let lane = -1;
      const up: LaneRow["up"] = [];
      const through: LaneRow["through"] = [];
      for (let i = 0; i < lanes.length; i++) {
        const waits = lanes[i];
        if (waits === null) continue;
        if (waits === oid) {
          if (lane < 0) lane = i;
          up.push({ from: i, color: this.colors[i] });
        } else {
          through.push({ lane: i, color: this.colors[i] });
        }
      }
      let color: number;
      if (lane < 0) {
        lane = this.freeLane(-1);
        color = this.open(lane, oid);
      } else {
        color = this.colors[lane];
        // The other lanes that waited for this commit end here.
        for (const u of up) if (u.from !== lane) lanes[u.from] = null;
      }
      let width = Math.max(lane, ...up.map((u) => u.from)) + 1;
      const down: LaneRow["down"] = [];
      if (parents.length === 0) {
        lanes[lane] = null;
      } else {
        lanes[lane] = parents[0];
        down.push({ to: lane, color });
        for (let p = 1; p < parents.length; p++) {
          const parent = parents[p];
          let target = -1;
          for (let i = 0; i < lanes.length; i++) {
            if (i !== lane && lanes[i] === parent) {
              target = i;
              break;
            }
          }
          let c: number;
          if (target >= 0) {
            c = this.colors[target];
          } else {
            target = this.freeLane(lane);
            c = this.open(target, parent);
          }
          down.push({ to: target, color: c });
          if (target + 1 > width) width = target + 1;
        }
      }
      for (const t of through) if (t.lane + 1 > width) width = t.lane + 1;
      while (lanes.length > 0 && lanes[lanes.length - 1] === null) {
        lanes.pop();
        this.colors.pop();
      }
      if (width > this.maxWidth) this.maxWidth = width;
      rows[n] = { lane, color, merge: parents.length > 1, through, up, down, width };
    }
    return rows;
  }
}

/** The rows of a whole history at once. */
export function layoutLanes(commits: readonly LaneCommit[]): LaneRow[] {
  return new LaneLayout().add(commits);
}

/** CSS colour of a lane colour index. */
export function laneColor(color: number): string {
  return `var(--rt-lane-${(color % LANE_COLORS) + 1})`;
}

/** SVG path data of one row (`x(lane)` is the column's centre, `h` the row height): lines that
 *  change columns are curves that leave and enter vertically. */
export function rowPaths(row: LaneRow, x: (lane: number) => number, h: number): { d: string; color: number }[] {
  const mid = h / 2;
  const out: { d: string; color: number }[] = [];
  for (const t of row.through) out.push({ d: `M${x(t.lane)} 0V${h}`, color: t.color });
  const cx = x(row.lane);
  for (const u of row.up) {
    const fx = x(u.from);
    out.push({ d: fx === cx ? `M${cx} 0V${mid}` : `M${fx} 0C${fx} ${mid * 0.9} ${cx} ${mid * 0.35} ${cx} ${mid}`, color: u.color });
  }
  for (const d of row.down) {
    const tx = x(d.to);
    out.push({ d: tx === cx ? `M${cx} ${mid}V${h}` : `M${cx} ${mid}C${cx} ${mid + mid * 0.65} ${tx} ${mid + mid * 0.1} ${tx} ${h}`, color: d.color });
  }
  return out;
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// The layout of the branch map (wave 8, strand E; V1 of docs/wellen/welle-8.md): the branches
// as lines of a transit map. Pure functions on commits and refs, no DOM, so the layout is tested
// and stays fast with 100 000 commits and 500 branches.
//
// Method. Every branch tip walks its first parents; a commit belongs to the first branch that
// reaches it (the default branch first, then the current one, then by the age of the tip, then by
// name), so the walk of a branch ends where it meets another branch: that is its fork point.
// Second parents of merge commits give the merge links. Only the *stations* get a place on the
// x axis (tips, fork points, merge points, the roots): they are ordered by commit time and spread
// evenly, which keeps a map of a hundred thousand commits as readable as one of ten. Lines are
// packed into rows (the default branch in row 0, the others around it, close to the branch they
// forked from, never two lines on one row where they overlap).

export interface MapCommit {
  oid: string;
  parents: readonly string[];
  /** Unix seconds. */
  time: number;
}

export interface MapRef {
  /** Short name as shown: `main`, `feature/login`. */
  name: string;
  /** The commit the branch points at. */
  target: string;
  current?: boolean;
  /** Commits ahead of / behind the upstream (the arrows at the end of the line). */
  ahead?: number;
  behind?: number;
  upstream?: string;
}

export interface BranchMapInput {
  /** Any order; commits that are not reachable from a ref are ignored. */
  commits: readonly MapCommit[];
  /** The local branches. */
  refs: readonly MapRef[];
  /** `main`, `master` or the branch the host names; default: the first of `main`, `master`, `trunk`, else the current one. */
  defaultBranch?: string;
  /** Now, Unix seconds (age of the tips). */
  now: number;
  /** Branches that are older than this many days are drawn dimmed; default 60. */
  dimAfterDays?: number;
  /** Branch → the branch it is stacked on (the product's stacks): those lines get the stack colour. */
  stacked?: Readonly<Record<string, string>>;
  /** Room the label after a line's tip needs, in slots (so no line starts under a label); default: from the name. */
  labelSlots?: (name: string) => number;
}

export interface MapStation {
  oid: string;
  /** Commit time, Unix seconds. */
  time: number;
  /** Index in the ordered stations. */
  index: number;
  x: number;
}

export interface MapLine {
  name: string;
  tip: string;
  /** Position in the colour cycle. */
  color: number;
  row: number;
  /** Commits of this line (the ones that belong to no other line). */
  commits: number;
  /** The station where it starts (its fork point), `null` for a root line. */
  fork: { oid: string; line: string } | null;
  /** x of the first and last station of the line. */
  from: number;
  to: number;
  current: boolean;
  ahead: number;
  behind: number;
  upstream?: string;
  /** Days since the tip. */
  age: number;
  dim: boolean;
  stacked: boolean;
  /** The line it is stacked on. */
  stackParent?: string;
  /** Stations on the line, in order. */
  stations: number[];
}

export interface MapLink {
  kind: "fork" | "merge";
  fromLine: string;
  toLine: string;
  from: { x: number; row: number };
  to: { x: number; row: number };
}

export interface BranchMapLayout {
  lines: MapLine[];
  links: MapLink[];
  stations: MapStation[];
  /** Rows from `minRow` to `maxRow` (negative: above the default branch). */
  minRow: number;
  maxRow: number;
  /** Number of x slots. */
  columns: number;
  /** The branch that is the trunk of the map. */
  trunk: string | null;
}

const DEFAULT_NAMES = ["main", "master", "trunk"];

export function pickTrunk(refs: readonly MapRef[], preferred?: string): MapRef | undefined {
  if (preferred) {
    const p = refs.find((r) => r.name === preferred);
    if (p) return p;
  }
  for (const name of DEFAULT_NAMES) {
    const r = refs.find((x) => x.name === name);
    if (r) return r;
  }
  return refs.find((r) => r.current) ?? refs[0];
}

/** The layout of the map. `x` and `row` are in slots; the component scales them. */
export function layoutBranchMap(input: BranchMapInput): BranchMapLayout {
  const empty: BranchMapLayout = { lines: [], links: [], stations: [], minRow: 0, maxRow: 0, columns: 0, trunk: null };
  const refs = input.refs.filter((r) => r.target);
  if (refs.length === 0) return empty;
  const byOid = new Map<string, MapCommit>();
  for (const c of input.commits) byOid.set(c.oid, c);

  const trunk = pickTrunk(refs, input.defaultBranch)!;
  const now = input.now;
  const tipTime = (r: MapRef) => byOid.get(r.target)?.time ?? 0;
  // Priority of the walks: trunk, current, newest tip, then name.
  const order = [...refs].sort((a, b) => {
    if (a === trunk) return -1;
    if (b === trunk) return 1;
    if (a.current !== b.current) return a.current ? -1 : 1;
    return tipTime(b) - tipTime(a) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
  });

  // 1. Ownership by first-parent walks.
  const owner = new Map<string, number>();
  const counts: number[] = [];
  const forks: (string | null)[] = [];
  order.forEach((ref, i) => {
    let n = 0;
    let at: string | undefined = ref.target;
    let fork: string | null = null;
    while (at !== undefined) {
      if (owner.has(at)) {
        fork = at;
        break;
      }
      const c = byOid.get(at);
      if (!c) break;
      owner.set(at, i);
      n++;
      at = c.parents[0];
    }
    counts.push(n);
    forks.push(fork);
  });

  // A branch whose tip is another branch's commit owns nothing: it points into that line.
  // It still gets a (short) line, forked at its own tip.

  // 2. Stations: tips, fork points, merge commits and the commit merged in.
  const wanted = new Set<string>();
  const merges: { from: string; to: string }[] = [];
  order.forEach((ref, i) => {
    wanted.add(ref.target);
    const f = forks[i];
    if (f) wanted.add(f);
  });
  for (const [oid, i] of owner) {
    const c = byOid.get(oid)!;
    for (let p = 1; p < c.parents.length; p++) {
      const parent = c.parents[p]!;
      if (!owner.has(parent)) continue;
      if (owner.get(parent) === i) continue;
      wanted.add(oid);
      wanted.add(parent);
      merges.push({ from: parent, to: oid });
    }
  }
  const ordered = [...wanted]
    .map((oid) => ({ oid, time: byOid.get(oid)?.time ?? 0 }))
    .sort((a, b) => a.time - b.time || (a.oid < b.oid ? -1 : 1));
  const stations: MapStation[] = ordered.map((s, index) => ({ oid: s.oid, time: s.time, index, x: index }));
  const xOf = new Map(stations.map((s) => [s.oid, s.x]));

  // 3. Lines with their span.
  const lineIndex = new Map(order.map((r, i) => [r.name, i]));
  const dimAfter = (input.dimAfterDays ?? 60) * 86_400;
  const lines: MapLine[] = order.map((ref, i) => {
    const fork = forks[i];
    const tipX = xOf.get(ref.target)!;
    const forkX = fork ? xOf.get(fork)! : tipX;
    const stackParent = input.stacked?.[ref.name];
    return {
      name: ref.name,
      tip: ref.target,
      color: i,
      row: 0,
      commits: counts[i]!,
      fork: fork ? { oid: fork, line: order[owner.get(fork)!]!.name } : null,
      from: Math.min(forkX, tipX),
      to: tipX,
      current: ref.current === true,
      ahead: ref.ahead ?? 0,
      behind: ref.behind ?? 0,
      ...(ref.upstream ? { upstream: ref.upstream } : {}),
      age: Math.max(0, Math.floor((now - tipTime(ref)) / 86_400)),
      dim: tipTime(ref) > 0 && now - tipTime(ref) > dimAfter && ref !== trunk && ref.current !== true,
      stacked: stackParent !== undefined,
      ...(stackParent !== undefined ? { stackParent } : {}),
      stations: [],
    };
  });
  // The trunk reaches back to its first commit.
  const root = lines[0]!;
  const trunkOldest = [...owner.entries()].filter(([, i]) => i === 0).map(([oid]) => xOf.get(oid)).filter((x): x is number => x !== undefined);
  if (trunkOldest.length > 0) root.from = Math.min(root.from, ...trunkOldest);

  // 4. Rows: the trunk in row 0, the rest by start, near the line they forked from.
  const rowsBusy = new Map<number, { from: number; to: number }[]>();
  const place = (line: MapLine, row: number) => {
    line.row = row;
    const list = rowsBusy.get(row) ?? [];
    list.push({ from: line.from, to: span(line) });
    rowsBusy.set(row, list);
  };
  const slots = input.labelSlots ?? ((name: string) => Math.ceil((name.length * 6.8 + 64) / 44));
  const span = (l: MapLine) => l.to + slots(l.name);
  const free = (row: number, line: MapLine) => (rowsBusy.get(row) ?? []).every((s) => span(line) < s.from || line.from > s.to);
  place(root, 0);
  const rest = lines.slice(1).sort((a, b) => a.from - b.from || a.to - b.to || (a.name < b.name ? -1 : 1));
  const rowOf = (name: string) => lines[lineIndex.get(name)!]?.row ?? 0;
  let above = 0;
  let below = 0;
  for (const line of rest) {
    const near = line.fork ? rowOf(line.fork.line) : 0;
    // Candidates by distance from the parent's row; ties alternate between below and above.
    let placed = false;
    for (let d = 0; d <= above + below + 2 && !placed; d++) {
      const candidates = d === 0 ? [near] : below <= above ? [near + d, near - d] : [near - d, near + d];
      for (const row of candidates) {
        if (row === 0 && line !== root) {
          // Row 0 is the trunk's, unless it ended before this line starts.
          if (!free(0, line)) continue;
        }
        if (free(row, line)) {
          place(line, row);
          placed = true;
          break;
        }
      }
    }
    if (!placed) place(line, below <= above ? below + 1 : -(above + 1));
    if (line.row > 0) below = Math.max(below, line.row);
    if (line.row < 0) above = Math.max(above, -line.row);
  }

  // 5. Links and the stations of each line.
  const links: MapLink[] = [];
  for (const line of lines) {
    if (!line.fork) continue;
    const parent = lines[lineIndex.get(line.fork.line)!]!;
    const x = xOf.get(line.fork.oid)!;
    links.push({ kind: "fork", fromLine: parent.name, toLine: line.name, from: { x, row: parent.row }, to: { x: Math.max(x, line.from), row: line.row } });
  }
  for (const m of merges) {
    const from = lines[owner.get(m.from)!]!;
    const to = lines[owner.get(m.to)!]!;
    links.push({ kind: "merge", fromLine: from.name, toLine: to.name, from: { x: xOf.get(m.from)!, row: from.row }, to: { x: xOf.get(m.to)!, row: to.row } });
  }
  const stationLines = new Map<number, Set<number>>();
  const add = (x: number, i: number) => stationLines.set(x, (stationLines.get(x) ?? new Set()).add(i));
  for (const [oid, i] of owner) {
    const x = xOf.get(oid);
    if (x !== undefined) add(x, i);
  }
  for (const [i, line] of lines.entries()) {
    add(xOf.get(line.tip)!, i);
    if (line.fork) add(xOf.get(line.fork.oid)!, i);
  }
  for (const [x, set] of stationLines) for (const i of set) lines[i]!.stations.push(x);
  for (const line of lines) line.stations.sort((a, b) => a - b);

  const rows = lines.map((l) => l.row);
  return {
    lines,
    links,
    stations,
    minRow: Math.min(0, ...rows),
    maxRow: Math.max(0, ...rows),
    columns: stations.length,
    trunk: trunk.name,
  };
}

/** One sentence for each line: the text form of the map. */
export function describeLine(line: MapLine, trunk: string | null): string {
  const parts: string[] = [];
  parts.push(line.name === trunk ? `${line.name} is the main line` : `${line.name} has ${line.commits} ${line.commits === 1 ? "commit" : "commits"} of its own`);
  if (line.fork && line.name !== trunk) parts.push(`forked from ${line.fork.line}`);
  if (line.stackParent) parts.push(`stacked on ${line.stackParent}`);
  if (line.ahead > 0) parts.push(`${line.ahead} to push`);
  if (line.behind > 0) parts.push(`${line.behind} to pull`);
  parts.push(line.age === 0 ? "updated today" : `last commit ${line.age} ${line.age === 1 ? "day" : "days"} ago`);
  if (line.current) parts.push("checked out");
  return `${parts.join(", ")}.`;
}

/** A synthetic input for the tests and the budget `viz_branch_map`: `branches` branches that
 *  fork from the main line, some of them merged back, some stacked, deterministic. */
export function syntheticBranchMap(branches: number, now = 1_800_000_000): BranchMapInput {
  const commits: MapCommit[] = [];
  const refs: MapRef[] = [];
  let t = now - 400 * 86_400;
  const tick = () => (t += 3_600);
  let main = "m0";
  commits.push({ oid: main, parents: [], time: tick() });
  let seq = 1;
  const mainChain: string[] = [main];
  for (let b = 0; b < branches; b++) {
    for (let k = 0; k < 2; k++) {
      const oid = `m${seq++}`;
      commits.push({ oid, parents: [main], time: tick() });
      main = oid;
      mainChain.push(oid);
    }
    let prev = main;
    const len = 1 + (b % 4);
    for (let k = 0; k < len; k++) {
      const oid = `f${b}-${k}`;
      commits.push({ oid, parents: [prev], time: tick() });
      prev = oid;
    }
    refs.push({ name: `feature/${String(b).padStart(3, "0")}`, target: prev, ahead: b % 5 === 0 ? 2 : 0, behind: b % 7 === 0 ? 1 : 0 });
    if (b % 3 === 0) {
      const oid = `m${seq++}`;
      commits.push({ oid, parents: [main, prev], time: tick() });
      main = oid;
      mainChain.push(oid);
    }
  }
  refs.unshift({ name: "main", target: main, current: true });
  return { commits, refs, now, defaultBranch: "main" };
}

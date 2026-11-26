// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: the graph's order and pages for the shells that build the graph in
// TypeScript (web on isomorphic-git, mock), with the same rules as `rt-git/src/graph.rs`:
// every commit reachable from the tips, children before parents, ties by commit time, newest
// first; refs sorted HEAD, local, remote, tag, each by name.

import type { GitChangedFile, GitGraphCommit, GitGraphPage, GitGraphQuery, GitRef, GitRefKind } from "../types";

export interface OrderCommit {
  oid: string;
  parents: string[];
  /** Commit time, Unix seconds. */
  time: number;
}

/** A max-heap on (time, -index). */
class Ready {
  private heap: [number, number][] = [];
  push(time: number, index: number): void {
    const h = this.heap;
    h.push([time, index]);
    let i = h.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (!Ready.before(h[i], h[p])) break;
      [h[i], h[p]] = [h[p], h[i]];
      i = p;
    }
  }
  pop(): number | undefined {
    const h = this.heap;
    if (h.length === 0) return undefined;
    const top = h[0][1];
    const last = h.pop()!;
    if (h.length > 0) {
      h[0] = last;
      let i = 0;
      for (;;) {
        const l = 2 * i + 1;
        const r = l + 1;
        let m = i;
        if (l < h.length && Ready.before(h[l], h[m])) m = l;
        if (r < h.length && Ready.before(h[r], h[m])) m = r;
        if (m === i) break;
        [h[i], h[m]] = [h[m], h[i]];
        i = m;
      }
    }
    return top;
  }
  private static before(a: [number, number], b: [number, number]): boolean {
    return a[0] !== b[0] ? a[0] > b[0] : a[1] < b[1];
  }
}

/** `commits` (any order; earlier wins ties) in date order: children before parents. Parents
 *  that are not in the list are ignored. */
export function dateOrder<T extends OrderCommit>(commits: readonly T[]): T[] {
  const index = new Map<string, number>();
  commits.forEach((c, i) => index.set(c.oid, i));
  const children = new Uint32Array(commits.length);
  for (const c of commits) {
    for (const p of c.parents) {
      const i = index.get(p);
      if (i !== undefined) children[i]++;
    }
  }
  const ready = new Ready();
  commits.forEach((c, i) => {
    if (children[i] === 0) ready.push(c.time, i);
  });
  const out: T[] = [];
  for (let i = ready.pop(); i !== undefined; i = ready.pop()) {
    const c = commits[i];
    out.push(c);
    for (const p of c.parents) {
      const j = index.get(p);
      if (j === undefined) continue;
      if (--children[j] === 0) ready.push(commits[j].time, j);
    }
  }
  return out;
}

/** The commits reachable from `tips` (in `commits`, any order). */
export function reachable<T extends OrderCommit>(commits: readonly T[], tips: readonly string[]): T[] {
  const byId = new Map(commits.map((c) => [c.oid, c]));
  const seen = new Set<string>();
  const todo = [...tips];
  while (todo.length > 0) {
    const oid = todo.pop()!;
    if (seen.has(oid)) continue;
    const c = byId.get(oid);
    if (!c) continue;
    seen.add(oid);
    todo.push(...c.parents);
  }
  return commits.filter((c) => seen.has(c.oid));
}

const KIND_ORDER: Record<GitRefKind, number> = { head: 0, local: 1, remote: 2, tag: 3 };

/** HEAD, local branches, remote branches, tags; each by name (byte order, like Rust). */
export function sortRefs(refs: GitRef[]): GitRef[] {
  return refs.sort((a, b) => KIND_ORDER[a.kind] - KIND_ORDER[b.kind] || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

export interface GraphSource {
  /** Every commit in date order (see [`dateOrder`]). */
  commits: readonly (OrderCommit & { summary: string; author: string; email: string; authorTime?: number })[];
  refs: GitRef[];
  head: string | null;
  detached: boolean;
}

/** One page of `source` (refs on every page, labels per commit). */
export function graphPage(source: GraphSource, query: GitGraphQuery): GitGraphPage {
  const skip = Math.max(0, query.skip ?? 0);
  const end = Math.min(source.commits.length, skip + Math.max(0, query.limit));
  const labels = new Map<string, string[]>();
  for (const r of source.refs) {
    const list = labels.get(r.target);
    if (list) list.push(r.full);
    else labels.set(r.target, [r.full]);
  }
  const commits: GitGraphCommit[] = [];
  for (let i = skip; i < end; i++) {
    const c = source.commits[i];
    const refs = labels.get(c.oid);
    commits.push({
      oid: c.oid,
      summary: c.summary,
      author: c.author,
      time: c.authorTime ?? c.time,
      parents: [...c.parents],
      email: c.email,
      ...(refs ? { refs } : {}),
    });
  }
  return {
    commits,
    refs: source.refs,
    ...(source.head ? { head: source.head } : {}),
    detached: source.detached,
    more: end < source.commits.length,
  };
}

/** Sorted by path, like the native shell. */
export function sortChanges(files: GitChangedFile[]): GitChangedFile[] {
  return files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand C: blame, history search and the reflog on the mock's Git (`GitApi.blame`,
// `searchHistory`, `reflog`). The mock keeps request files, so "files" here are the requests of
// the demo collection, as the text `renderSpec` gives them; paths are `/<request file>`.
//
// The reflog is made from what the mock knows: the actions in its journal (every move of a ref
// they made, newest first) and below them the first-parent history of the ref, as "commit:"
// entries. Like Git's own, it can show commits no branch points to any more.

import { BackendError } from "./backend";
import { lineDiff, renderSpec, type MockGit } from "./mock-git";
import type { Core } from "./mock-git-actions";
import type {
  GitBlame,
  GitBlameHunk,
  GitCommit,
  GitDiff,
  GitGraphCommit,
  GitHistoryField,
  GitHistoryHit,
  GitHistoryPage,
  GitHistoryQuery,
  GitReflogEntry,
  RequestSpec,
} from "./types";
import type { GitHistoryMethods } from "./git-methods";
import { dateOrder } from "../git/graph/order";

type Tree = Map<string, RequestSpec>;

const err = (message: string) => new BackendError("git", message);
const unslash = (p: string) => p.replace(/^\/+/, "");
const HIT_PATHS = 5;

/** Lines of a text, without the final line break. */
const linesOf = (text: string | null): string[] => (text ? text.replace(/\n$/, "").split("\n") : []);

/** Where each line of the new text is in the old text (`-1`: the diff added it). */
function unchanged(diff: GitDiff, newLines: number): number[] {
  const map = new Array<number>(newLines).fill(-1);
  let o = 0;
  let n = 0;
  const same = (until: number) => {
    while (n < until) {
      if (n < newLines) map[n] = o;
      o++;
      n++;
    }
  };
  for (const h of diff.hunks) {
    same(h.new_lines === 0 ? h.new_start : h.new_start - 1);
    for (const line of h.lines) {
      if (line.kind === "context") {
        if (n < newLines) map[n] = o;
        o++;
        n++;
      } else if (line.kind === "added") n++;
      else o++;
    }
  }
  same(newLines);
  return map;
}

export function mockGitHistory(git: MockGit): GitHistoryMethods {
  const c = git as unknown as Core;
  const text = (tree: Tree, path: string) => {
    const spec = tree.get(path);
    return spec ? renderSpec(spec) : null;
  };
  const resolve = (rev: string): string => {
    c.start();
    try {
      return c.resolveRev(rev);
    } catch {
      throw err(`Unknown revision "${rev}".`);
    }
  };

  // ---- blame ----
  async function blame(rev: string, path: string): Promise<GitBlame> {
    c.calls.push(`blame ${rev} ${path}`);
    const oid = resolve(rev);
    const file = unslash(path);
    if (!file || file.split("/").some((p) => p === ".." || p === "." || p === "")) throw err(`Invalid path "${path}".`);
    const mine = text(c.store.get(oid)!.tree, file);
    if (mine === null) throw err(`/${file} does not exist in ${rev}`);
    const total = linesOf(mine).length;
    const out: GitBlame = { path: `/${file}`, rev: oid, lines: total, hunks: [], binary: false };
    const blamed: ([string, number] | null)[] = new Array<[string, number] | null>(total).fill(null);
    // Newest first: [commit, lines as [final index, index in the commit]].
    const pending = new Map<string, [number, number][]>();
    const push = (id: string, lines: [number, number][]) => pending.set(id, [...(pending.get(id) ?? []), ...lines]);
    push(oid, Array.from({ length: total }, (_, i): [number, number] => [i, i]));
    while (pending.size > 0) {
      const [id] = [...pending.keys()].sort((a, b) => c.store.get(b)!.time - c.store.get(a)!.time);
      const lines = pending.get(id)!;
      pending.delete(id);
      const commit = c.store.get(id)!;
      const now = text(commit.tree, file)!;
      let remaining = lines;
      for (const parent of commit.parents) {
        if (remaining.length === 0) break;
        const before = text(c.store.get(parent)!.tree, file);
        if (before === null) continue;
        const map = before === now ? null : unchanged(lineDiff(file, before, now), linesOf(now).length);
        const pass: [number, number][] = [];
        const keep: [number, number][] = [];
        for (const [fin, cur] of remaining) {
          const old = map ? map[cur]! : cur;
          if (old >= 0) pass.push([fin, old]);
          else keep.push([fin, cur]);
        }
        remaining = keep;
        if (pass.length > 0) push(parent, pass);
      }
      for (const [fin, cur] of remaining) blamed[fin] = [id, cur];
    }
    let run: { oid: string; orig: number; len: number } | null = null;
    let start = 1;
    const flush = () => {
      if (!run) return;
      const hunk: GitBlameHunk = { start, lines: run.len, commit: c.toCommit(run.oid), path: `/${file}`, orig_start: run.orig + 1 };
      out.hunks.push(hunk);
      start += run.len;
      run = null;
    };
    for (const b of blamed) {
      const [id, cur] = b!;
      if (run && run.oid === id && run.orig + run.len === cur) run.len++;
      else {
        flush();
        run = { oid: id, orig: cur, len: 1 };
      }
    }
    flush();
    return out;
  }

  // ---- search ----
  function labels(): Map<string, string[]> {
    const data = git.graphData();
    const map = new Map<string, string[]>();
    const add = (oid: string | null, ref: string) => {
      if (oid) map.set(oid, [...(map.get(oid) ?? []), ref]);
    };
    for (const b of data.branches) add(b.oid, `refs/heads/${b.name}`);
    for (const r of data.remote) add(r.oid, `refs/remotes/${r.name}`);
    for (const t of data.tags) add(t.oid, `refs/tags/${t.name}`);
    return map;
  }

  /** Lines of `text` with `needle` in them, with their counts. */
  function needleLines(t: string | null, needle: string): Map<string, number> {
    const m = new Map<string, number>();
    for (const l of linesOf(t)) {
      const lower = l.toLowerCase();
      if (lower.includes(needle)) m.set(lower, (m.get(lower) ?? 0) + 1);
    }
    return m;
  }

  async function searchHistory(query: GitHistoryQuery): Promise<GitHistoryPage> {
    c.calls.push(`searchHistory ${JSON.stringify(query)}`);
    c.start();
    const data = git.graphData();
    const tips = query.refs
      ? query.refs.map((r) => {
          const hit = data.branches.find((b) => `refs/heads/${b.name}` === r);
          return hit?.oid ?? resolve(r);
        })
      : [...data.branches.map((b) => b.oid), ...data.remote.map((r) => r.oid), ...data.tags.map((t) => t.oid), c.head()].filter((o): o is string => Boolean(o));
    const seen = new Set<string>();
    const todo = [...tips];
    while (todo.length > 0) {
      const o = todo.pop()!;
      if (seen.has(o)) continue;
      seen.add(o);
      todo.push(...(c.store.get(o)?.parents ?? []));
    }
    const order = dateOrder(
      data.commits.filter((x) => seen.has(x.oid)).map((x) => ({ oid: x.oid, parents: x.parents, time: x.time, message: x.message, author: x.author })),
    );
    const lower = (s?: string) => (s ? s.toLowerCase() : "");
    const [message, author, content] = [lower(query.message), lower(query.author), lower(query.content)];
    const want = query.path ? (query.path.startsWith("/") ? query.path : `/${query.path}`) : "";
    const refs = labels();
    const page: GitHistoryPage = { hits: [], more: false, scanned: 0 };
    let hits = 0;
    for (const row of order) {
      page.scanned++;
      if ((query.since != null && row.time < query.since) || (query.until != null && row.time > query.until)) continue;
      const matched: GitHistoryField[] = [];
      const email = `${row.author.split(" ")[0]!.toLowerCase()}@example.com`;
      if (message) {
        if (!row.message.toLowerCase().includes(message)) continue;
        matched.push("message");
      }
      if (author) {
        if (!`${row.author} <${email}>`.toLowerCase().includes(author)) continue;
        matched.push("author");
      }
      let paths: string[] = [];
      if (want || content) {
        if (row.parents.length > 1) continue;
        const now = c.store.get(row.oid)!.tree;
        const before = row.parents[0] ? c.store.get(row.parents[0])!.tree : new Map<string, RequestSpec>();
        const changed = [...new Set([...now.keys(), ...before.keys()])].filter((p) => text(now, p) !== text(before, p) && `/${p}`.startsWith(want || "/")).sort();
        if (changed.length === 0) continue;
        if (want) matched.push("path");
        if (content) {
          const total = new Map<string, number>();
          const files: string[] = [];
          for (const p of changed) {
            const delta = new Map<string, number>();
            for (const [l, n] of needleLines(text(now, p), content)) delta.set(l, (delta.get(l) ?? 0) + n);
            for (const [l, n] of needleLines(text(before, p), content)) delta.set(l, (delta.get(l) ?? 0) - n);
            let any = false;
            for (const [l, n] of delta) {
              if (n === 0) continue;
              any = true;
              total.set(l, (total.get(l) ?? 0) + n);
            }
            if (any) files.push(`/${p}`);
          }
          if ([...total.values()].every((n) => n === 0)) continue;
          matched.push("content");
          paths = files.slice(0, HIT_PATHS);
        } else paths = changed.slice(0, HIT_PATHS).map((p) => `/${p}`);
      }
      hits++;
      if (hits <= (query.skip ?? 0)) continue;
      if (page.hits.length >= query.limit) {
        page.more = true;
        break;
      }
      const commit: GitGraphCommit = {
        oid: row.oid,
        summary: row.message.split("\n")[0]!,
        author: row.author,
        time: row.time,
        parents: [...row.parents],
        email,
        ...(refs.get(row.oid) ? { refs: refs.get(row.oid)! } : {}),
      };
      const hit: GitHistoryHit = { commit, matched, ...(paths.length > 0 ? { paths } : {}) };
      page.hits.push(hit);
    }
    return page;
  }

  // ---- reflog ----
  async function reflog(name: string, limit: number): Promise<GitReflogEntry[]> {
    c.calls.push(`reflog ${name}`);
    c.start();
    const wanted = name.trim();
    if (!wanted) throw err("No ref given.");
    const full = wanted === "HEAD" || wanted.startsWith("refs/") ? wanted : `refs/heads/${wanted}`;
    const branch = full === "HEAD" ? (c.current ? `refs/heads/${c.current}` : null) : full;
    const value = (ref: string | null): string | null => (ref === null ? c.head() : ref.startsWith("refs/heads/") ? (c.branchesMap.get(ref.slice(11)) ?? null) : null);
    const out: GitReflogEntry[] = [];
    const entry = (old: string | null, neu: string, message: string, time: number): void => {
      const commit: GitCommit | undefined = c.store.has(neu) ? c.toCommit(neu) : undefined;
      out.push({ name: full, index: out.length, old, new: neu, message, time, ...(commit ? { commit } : {}) });
    };
    // What the actions did, newest first.
    let tip = value(branch);
    for (const j of c.journal) {
      const move = j.entry.refs.find((r) => r.name === full || r.name === branch);
      if (!move || move.after === null) continue;
      entry(move.before, move.after, `${j.entry.action}: ${j.entry.summary}`, j.entry.time);
      tip = move.before;
    }
    // Below them, the commits themselves along the first parents.
    for (let oid = tip; oid; ) {
      const commit = c.store.get(oid);
      if (!commit) break;
      const parent = commit.parents[0] ?? null;
      entry(parent, oid, `commit: ${commit.message.split("\n")[0]}`, commit.time);
      oid = parent;
    }
    return out.slice(0, limit).map((e, index) => ({ ...e, index }));
  }

  return { blame, searchHistory, reflog };
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: the commit graph of the mock's Git (`GitApi.graph`, `refs`, `changes`,
// `fileDiff`), from `MockGit.graphData()` (copies, never the private fields) and the public
// revision methods. The mock's repository root is the demo collection, so every request file
// is a file of that collection.
//
// `?backend=mock&graph=<n>` replaces the history with a synthetic one of `n` commits
// (`git/graph/synthetic.ts`) for the graph budgets and for looking at a large graph; its files
// are documents and code, shown as line diffs.

import { BackendError } from "./backend";
import { DEMO_ID } from "./mock-collections";
import { lineDiff, type MockGit } from "./mock-git";
import type { GitChangedFile, GitDiff, GitFileChange, GitRef, RequestSpec } from "./types";
import type { GitGraphMethods } from "./wave14-stubs";
import { dateOrder, graphPage, reachable, sortChanges, sortRefs, syntheticRepo, type GraphSource } from "@tailor-kit/git-ui";

/** What `graphData()` has since strand G (tags, a detached `HEAD`); optional until then. */
interface GraphDataExtra {
  tags?: { name: string; oid: string; message: string | null }[];
  detached?: string | null;
}

/** `rt_app::git::tree_path` for the mock's request files. */
function collectionFile(path: string): boolean {
  const parts = path.split("/");
  const file = parts.pop() ?? "";
  return file.endsWith(".yml") && parts.every((p) => p !== "" && !p.startsWith(".") && p !== "node_modules");
}

function ancestors(parents: Map<string, string[]>, oid: string): Set<string> {
  const seen = new Set<string>();
  const todo = [oid];
  while (todo.length > 0) {
    const o = todo.pop()!;
    if (seen.has(o)) continue;
    seen.add(o);
    todo.push(...(parents.get(o) ?? []));
  }
  return seen;
}

function onlyIn(parents: Map<string, string[]>, a: string, b: string): number {
  const other = ancestors(parents, b);
  let n = 0;
  for (const o of ancestors(parents, a)) if (!other.has(o)) n++;
  return n;
}

function refsOf(data: ReturnType<MockGit["graphData"]> & GraphDataExtra): { refs: GitRef[]; head: string | null; detached: boolean } {
  const parents = new Map(data.commits.map((c) => [c.oid, c.parents]));
  const remote = new Map(data.remote.map((r) => [r.name, r.oid]));
  const refs: GitRef[] = [];
  for (const b of data.branches) {
    if (!b.oid) continue;
    const up = remote.get(`origin/${b.name}`);
    refs.push({
      full: `refs/heads/${b.name}`,
      name: b.name,
      kind: "local",
      target: b.oid,
      current: b.name === data.current,
      ...(up ? { upstream: `origin/${b.name}` } : {}),
      ahead: up ? onlyIn(parents, b.oid, up) : 0,
      behind: up ? onlyIn(parents, up, b.oid) : 0,
    });
  }
  for (const r of data.remote) {
    refs.push({ full: `refs/remotes/${r.name}`, name: r.name, kind: "remote", target: r.oid, current: false, ahead: 0, behind: 0 });
  }
  for (const t of data.tags ?? []) {
    refs.push({ full: `refs/tags/${t.name}`, name: t.name, kind: "tag", target: t.oid, current: false, ahead: 0, behind: 0, ...(t.message ? { message: t.message } : {}) });
  }
  const branchHead = data.current ? (data.branches.find((b) => b.name === data.current)?.oid ?? null) : null;
  const detachedAt = data.current ? null : (data.detached ?? null);
  if (detachedAt) refs.push({ full: "HEAD", name: "HEAD", kind: "head", target: detachedAt, current: true, ahead: 0, behind: 0 });
  return { refs: sortRefs(refs), head: branchHead ?? detachedAt, detached: detachedAt !== null };
}

function fromRepo(git: MockGit): GraphSource {
  const data = git.graphData() as ReturnType<MockGit["graphData"]> & GraphDataExtra;
  // `scenario("none")`: no branch, no HEAD, nothing to show; the native and web backends fail
  // the same way outside a repository.
  if (data.current === null && !data.detached && data.branches.length === 0) {
    throw new BackendError("git", "The collection is not in a Git repository.");
  }
  const { refs, head, detached } = refsOf(data);
  // In the mock a commit's author is a display name; derive a stable address for the avatar.
  const commits = dateOrder(
    data.commits.map((c) => ({
      oid: c.oid,
      parents: c.parents,
      time: c.time,
      summary: c.message.split("\n")[0],
      author: c.author,
      email: `${c.author.split(" ")[0].toLowerCase()}@example.com`,
    })),
  );
  return { commits, refs, head, detached };
}

function synthetic(count: number): GraphSource {
  const repo = syntheticRepo(count, 14);
  const main = repo.branches[0].oid;
  const firstParent = repo.commits.find((c) => c.oid === main)?.parents[0] ?? main;
  const refs: GitRef[] = [
    ...repo.branches.map((b, i): GitRef => ({
      full: `refs/heads/${b.name}`,
      name: b.name,
      kind: "local",
      target: b.oid,
      current: i === 0,
      ...(i === 0 ? { upstream: "origin/main" } : {}),
      ahead: i === 0 && firstParent !== main ? 1 : 0,
      behind: 0,
    })),
    { full: "refs/remotes/origin/main", name: "origin/main", kind: "remote", target: firstParent, current: false, ahead: 0, behind: 0 },
    ...repo.tags.map((t): GitRef => ({ full: `refs/tags/${t.name}`, name: t.name, kind: "tag", target: t.oid, current: false, ahead: 0, behind: 0, ...(t.message ? { message: t.message } : {}) })),
  ];
  return { commits: repo.commits, refs: sortRefs(refs), head: main, detached: false };
}

const SYNTHETIC_FILES: { path: string; change: GitFileChange; binary?: boolean }[] = [
  { path: "/README.md", change: "modified" },
  { path: "/openapi.yaml", change: "modified" },
  { path: "/docs/guide.md", change: "added" },
  { path: "/scripts/seed.mjs", change: "modified" },
  { path: "/assets/logo.png", change: "modified", binary: true },
];

const textOf = (spec: RequestSpec | undefined | null) => (spec ? JSON.stringify(spec, null, 2) : null);

export function mockGitGraph(git: MockGit): GitGraphMethods {
  const wanted = Number(new URLSearchParams(globalThis.location?.search ?? "").get("graph") ?? "");
  const big = Number.isInteger(wanted) && wanted >= 2 ? synthetic(wanted) : null;
  const source = (): GraphSource => big ?? fromRepo(git);
  const known = (src: GraphSource, rev: string) => src.commits.some((c) => c.oid === rev);
  const resolve = async (src: GraphSource, rev: string): Promise<string> => {
    if (known(src, rev)) return rev;
    const ref = src.refs.find((r) => r.full === rev || r.name === rev);
    if (ref) return ref.target;
    if (big) {
      if (rev === "HEAD" && src.head) return src.head;
      throw new BackendError("git", `Unknown revision "${rev}".`);
    }
    return git.revParse(rev);
  };

  return {
    async graph(query) {
      const src = source();
      if (!query.refs) return graphPage(src, query);
      const tips = await Promise.all(query.refs.map((r) => resolve(src, r)));
      return graphPage({ ...src, commits: dateOrder(reachable(src.commits, tips)) }, query);
    },

    async refs() {
      return source().refs;
    },

    async changes(from, to) {
      const src = source();
      const target = await resolve(src, to);
      const commit = src.commits.find((c) => c.oid === target);
      if (!commit) throw new BackendError("git", `Unknown revision "${to}".`);
      if (big) {
        const n = parseInt(target.slice(0, 4), 16);
        const files = SYNTHETIC_FILES.filter((_, i) => (n >> i) & 1 || i === n % SYNTHETIC_FILES.length);
        return sortChanges(files.map((f) => ({ path: f.path, change: f.change, binary: Boolean(f.binary) })));
      }
      const base = from ? await resolve(src, from) : (commit.parents[0] ?? null);
      const before = base ? git.specsAt(base) : new Map<string, RequestSpec>();
      const after = git.specsAt(target);
      const out: GitChangedFile[] = [];
      const file = (path: string, change: GitFileChange): GitChangedFile => ({
        path: `/${path}`,
        change,
        binary: false,
        ...(collectionFile(path) ? { collection: DEMO_ID } : {}),
      });
      for (const [path, spec] of after) {
        const old = before.get(path);
        if (!old) out.push(file(path, "added"));
        else if (textOf(old) !== textOf(spec)) out.push(file(path, "modified"));
      }
      for (const path of before.keys()) if (!after.has(path)) out.push(file(path, "deleted"));
      return sortChanges(out);
    },

    async fileDiff(from, to, path): Promise<GitDiff> {
      const src = source();
      if (big) {
        const old = `# Notes\n\nLine one.\nLine two.\n`;
        return lineDiff(path, old, `${old}Line three (${(to ?? "work").slice(0, 7)}).\n`);
      }
      const rel = path.replace(/^\//, "");
      if (to === null) return { ...(await git.diff(rel)), path };
      const target = await resolve(src, to);
      const base = from ? await resolve(src, from) : (src.commits.find((c) => c.oid === target)?.parents[0] ?? null);
      const old = base ? await git.show(base, rel) : null;
      return lineDiff(path, old, await git.show(target, rel));
    },
  };
}

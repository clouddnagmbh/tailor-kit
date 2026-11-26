// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// Git in the mock backend, for the Git UI's tests and UI development. Owner: wave 4, strand A2;
// moved to `@tailor-kit/git-ui/mock` in step K3.
//
// A small, deterministic, in-memory Git: commits with trees of files, local branches,
// one remote ("origin") that tests can move ahead (`remoteCommit`), fetch/merge with
// conflicts, credentials that can be required per host (`requireAuth`), and scripted
// scenarios (`scenario("conflict")`). The working tree is the host's saved files
// (`MockGitHost.saved`). `checkout`, `pull` and `resolve` rewrite the host's files, so the
// tree in the UI changes like on disk. Without a host (unit tests) it keeps its own working tree.
//
// What a file is, and the fixtures (scenarios, the first commit, the other files of the demo
// repository) belong to the product: `MockContent<S>` and `MockFixtures<S>` (./content.ts). The
// defaults are plain text files. Starts as a repository on `main` with one commit of all files,
// in sync with origin.

import { BackendError, type GitApi, type GitOwnerInfo } from "../api";
import { LOCAL_CLONE_UNSUPPORTED, looksLocal } from "../clone-source";
import type { DiffHunk, DiffLine, GitBranch, GitCommit, GitConflict, GitDiff, GitFile, GitFileCommit, GitIdentity, GitPullResult, GitResolution, GitStatus, GitTree, SecretFinding } from "../types";
import { mockGitActions, type Core, type MockJournalEntry, type MockStash } from "./actions";
import { textContent, textFixtures, type MockContent, type MockFixtures, type MockGitScenario } from "./content";
import { mockGitFiles } from "./files";
import { mockGitGraph } from "./graph";
import { mockGitHistory } from "./history";
import { mockGitIndex } from "./staging";
import { mockGitPlumbing } from "./plumbing";
import { mockRefusal, mockScan } from "./secret-scan";

/** What the mock Git needs from the mock backend of the product. */
export interface MockGitHost<S = string> {
  /** The files in memory (the editor state). */
  live(): Map<string, S>;
  /** The files as saved "on disk": the working tree. */
  saved(): Map<string, S>;
  /** Paths of unsaved edits. */
  dirty(): Promise<string[]>;
  /** What a clone opens: the product's owner of the files, or `null`. */
  openOwner(): Promise<GitOwnerInfo | null>;
}

/** What `new MockGit` takes. `content` and `fixtures` default to plain text files. */
export interface MockGitOptions<S = string> {
  host?: MockGitHost<S>;
  /** Milliseconds since the epoch; the first commit is an hour before it. */
  clock?: () => number;
  content?: MockContent<S>;
  fixtures?: MockFixtures<S>;
}

export type Tree<S> = Map<string, S>;

export interface Commit<S> {
  oid: string;
  parents: string[];
  tree: Tree<S>;
  message: string;
  author: string;
  time: number;
}

export const MOCK_REMOTE_URL = "https://git.example.com/acme/demo-api.git";
export const MOCK_REMOTE_HOST = "git.example.com";
const CONTEXT = 3;

const err = (message: string) => new BackendError("git", message);
const clone = <T>(v: T): T => structuredClone(v);

/** Line diff with `CONTEXT` lines of context, in the shape of `rt_app::git::text_diff`. */
export function lineDiff(path: string, old: string | null, current: string | null): GitDiff {
  const split = (t: string | null) => (t ? t.replace(/\n$/, "").split("\n") : []);
  const a = split(old);
  const b = split(current);
  // LCS table (files in the mock are small).
  const lcs = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  interface Op {
    line: DiffLine;
    oldAt: number;
    newAt: number;
  }
  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      ops.push({ line: { kind: "context", text: a[i] }, oldAt: i++, newAt: j++ });
    } else if (i < a.length && (j === b.length || lcs[i + 1][j] >= lcs[i][j + 1])) {
      ops.push({ line: { kind: "removed", text: a[i] }, oldAt: i++, newAt: j });
    } else {
      ops.push({ line: { kind: "added", text: b[j] }, oldAt: i, newAt: j++ });
    }
  }
  const changed = ops.map((o, k) => (o.line.kind === "context" ? -1 : k)).filter((k) => k >= 0);
  const hunks: DiffHunk[] = [];
  let k = 0;
  while (k < changed.length) {
    const from = Math.max(0, changed[k] - CONTEXT);
    let to = changed[k];
    while (k + 1 < changed.length && changed[k + 1] - to <= 2 * CONTEXT) to = changed[++k];
    to = Math.min(ops.length - 1, to + CONTEXT);
    k++;
    const slice = ops.slice(from, to + 1);
    const oldLines = slice.filter((o) => o.line.kind !== "added").length;
    const newLines = slice.filter((o) => o.line.kind !== "removed").length;
    const oldStart = slice.find((o) => o.line.kind !== "added")?.oldAt ?? slice[0].oldAt;
    const newStart = slice.find((o) => o.line.kind !== "removed")?.newAt ?? slice[0].newAt;
    hunks.push({
      old_start: oldLines === 0 ? oldStart : oldStart + 1,
      old_lines: oldLines,
      new_start: newLines === 0 ? newStart : newStart + 1,
      new_lines: newLines,
      lines: slice.map((o) => o.line),
    });
  }
  return { path, binary: false, hunks };
}

/** The mock's Git, with test helpers beyond `GitApi`. */
export class MockGit<S = string> implements GitApi {
  private own: Tree<S> = new Map();
  private isRepo = true;
  private store = new Map<string, Commit<S>>();
  private branchesMap = new Map<string, string | null>();
  private current: string | null = "main";
  /** The commit `HEAD` points to while it is on no branch (`current` is then `null`). */
  private detachedAt: string | null = null;
  /** Tags: name → commit, and the message of annotated ones. */
  private tagsMap = new Map<string, { oid: string; message: string | null }>();
  private tracking = new Set<string>();
  /** `refs/remotes/origin/*` as last fetched. */
  private fetched = new Map<string, string>();
  /** The server's branches. */
  private remote: Map<string, string> | null = new Map();
  private mergeHead: string | null = null;
  // Wave 14 (strand G, used by mock-git-actions.ts): what the merge in progress is, the base of
  // a pick/revert and its commit message; remotes by name; tags on the server; parked changes;
  // the action journal and the backup refs.
  private mergeKind: "merge" | "cherry-pick" | "revert" | null = null;
  private mergeBaseOid: string | null = null;
  private mergeMessage: string | null = null;
  private remoteUrls = new Map<string, string>();
  private remoteTags = new Set<string>();
  private stashList: MockStash[] = [];
  private journal: MockJournalEntry<S>[] = [];
  private backups = new Map<string, string>();
  private conflicts = new Set<string>();
  private counter = 0;
  private who: GitIdentity | null = null;
  private credentials = new Map<string, { username: string; password: string }>();
  private authRequired = new Set<string>();
  /** Every Git command the UI called, for tests. */
  readonly calls: string[] = [];

  /** The first state is made on first use: the backend fills its requests after creating
   *  this object. */
  private started = false;

  /** Unix seconds of the first commit: the clock at construction. Later commits follow a minute
   *  apart, so the order is stable and `ago()` shows plausible times. */
  private readonly startTime: number;

  private readonly host: MockGitHost<S> | undefined;
  /** What a file is (shown text, equality, merging). */
  readonly content: MockContent<S>;
  /** The product's scenarios and demo files. */
  readonly fixtures: MockFixtures<S>;

  constructor(options: MockGitOptions<S> = {}) {
    this.host = options.host;
    this.content = options.content ?? (textContent as unknown as MockContent<S>);
    this.fixtures = options.fixtures ?? (textFixtures as unknown as MockFixtures<S>);
    this.startTime = Math.floor((options.clock ?? Date.now)() / 1000) - 3_600;
    // After `content` and `fixtures` are set: the parts read them.
    this.wave14Graph = mockGitGraph(this);
    this.wave14Actions = mockGitActions(this);
    this.wave15Working = mockGitFiles(this);
    this.wave15History = mockGitHistory(this);
    this.wave15Plumbing = mockGitPlumbing(this);
    this.wave15Index = mockGitIndex(this);
  }

  private start(): void {
    if (!this.started) this.scenario("clean");
  }

  /** The saved files of the working tree (a copy). */
  savedFiles(): Map<string, S> {
    this.start();
    return clone(this.work);
  }

  private get work(): Tree<S> {
    return this.host?.saved() ?? this.own;
  }

  // ---- test helpers ----

  /** Resets to a known state. `none`: not a repository. `clean`: one commit, in sync with
   *  origin. The others are on top of `clean` and defined by the fixtures: `conflict` (a local
   *  and a remote commit change the same file, so the next pull stops with a conflict) and the
   *  states of the Git panel: `empty` (a repository without commits), `no-remote`, `unpublished`
   *  (a remote, the branch not on it), `ahead`, `behind` (fetched already), `diverged`,
   *  `detached` (HEAD on the first commit) and `showcase` (branches, a merge, a tag, ahead by 2
   *  and three changed files). */
  scenario(name: MockGitScenario): void {
    this.started = true;
    this.store.clear();
    this.branchesMap.clear();
    this.tagsMap.clear();
    this.detachedAt = null;
    this.tracking.clear();
    this.fetched.clear();
    this.mergeHead = null;
    this.mergeKind = null;
    this.mergeBaseOid = null;
    this.mergeMessage = null;
    this.conflicts.clear();
    this.counter = 0;
    this.remoteUrls.clear();
    this.remoteTags.clear();
    this.stashList = [];
    this.journal = [];
    this.backups.clear();
    if (!this.host && this.own.size === 0) this.own = this.fixtures.initialFiles();
    if (name === "none") {
      this.isRepo = false;
      this.current = null;
      this.remote = null;
      return;
    }
    this.isRepo = true;
    this.current = "main";
    const first = this.newCommit([], clone(this.work), this.fixtures.initialMessage, "Demo Author");
    this.branchesMap.set("main", first);
    this.remote = new Map([["main", first]]);
    this.remoteUrls.set("origin", MOCK_REMOTE_URL);
    this.fetched.set("main", first);
    this.tracking.add("main");
    this.fixtures.scenario(this as unknown as Core<S>, name, first);
  }

  /** A commit of `files` on the current branch, written to the working tree too. */
  localCommit(files: Record<string, S | null>, message: string, author: string): string {
    const parent = this.head();
    const tree: Tree<S> = parent ? clone(this.store.get(parent)!.tree) : new Map();
    for (const [p, spec] of Object.entries(files)) {
      if (spec) tree.set(p, clone(spec));
      else tree.delete(p);
      this.writeWork(p, spec ?? undefined);
    }
    const oid = this.newCommit(parent ? [parent] : [], tree, message, author);
    this.branchesMap.set(this.current!, oid);
    return oid;
  }

  /** A commit on the remote's `branch` (another person's push). `null` deletes a file. */
  remoteCommit(files: Record<string, S | null>, message = "Remote change", branch = "main"): string {
    this.start();
    if (!this.remote) throw new Error("mock git: no remote");
    const parent = this.remote.get(branch);
    const tree: Tree<S> = parent ? clone(this.store.get(parent)!.tree) : new Map();
    for (const [p, spec] of Object.entries(files)) {
      if (spec) tree.set(p, clone(spec));
      else tree.delete(p);
    }
    const oid = this.newCommit(parent ? [parent] : [], tree, message, "Colleague");
    this.remote.set(branch, oid);
    return oid;
  }

  /** Push, pull and clone against `host` fail with `git-auth` until credentials are set. */
  requireAuth(host = MOCK_REMOTE_HOST): void {
    this.authRequired.add(host);
  }

  // ---- internals ----

  newCommit(parents: string[], tree: Tree<S>, message: string, author: string): string {
    this.counter++;
    let h = 0x811c9dc5;
    const seed = `${this.counter}:${message}:${parents.join(",")}`;
    let oid = "";
    for (let round = 0; oid.length < 40; round++) {
      for (const c of `${seed}:${round}`) h = Math.imul(h ^ c.charCodeAt(0), 0x01000193) >>> 0;
      oid += h.toString(16).padStart(8, "0");
    }
    oid = oid.slice(0, 40);
    this.store.set(oid, { oid, parents, tree, message, author, time: this.startTime + 60 * this.counter });
    return oid;
  }

  private head(): string | null {
    return this.current ? (this.branchesMap.get(this.current) ?? null) : this.detachedAt;
  }

  headTree(): Tree<S> {
    const h = this.head();
    return h ? this.store.get(h)!.tree : new Map();
  }

  private ancestors(oid: string): Set<string> {
    const seen = new Set<string>();
    const todo = [oid];
    while (todo.length) {
      const o = todo.pop()!;
      if (seen.has(o)) continue;
      seen.add(o);
      todo.push(...this.store.get(o)!.parents);
    }
    return seen;
  }

  private countOnly(from: string, not: string | null): number {
    const exclude = not ? this.ancestors(not) : new Set<string>();
    return [...this.ancestors(from)].filter((o) => !exclude.has(o)).length;
  }

  private baseOf(a: string, b: string): string | null {
    const ofB = this.ancestors(b);
    // Nearest common ancestor by commit order (the mock's histories are small).
    const common = [...this.ancestors(a)].filter((o) => ofB.has(o));
    common.sort((x, y) => this.store.get(y)!.time - this.store.get(x)!.time);
    return common[0] ?? null;
  }

  writeWork(path: string, spec: S | undefined): void {
    const targets = this.host ? [this.host.saved(), this.host.live()] : [this.own];
    for (const t of targets) {
      if (spec) t.set(path, clone(spec));
      else t.delete(path);
    }
  }

  private requireRepo(): void {
    if (!this.isRepo) throw err(`The ${this.fixtures.scope} is not in a Git repository.`);
  }

  /** Unsaved edits in the app (none without a host). */
  private async dirtyFiles(): Promise<string[]> {
    return this.host ? this.host.dirty() : [];
  }

  private async requireSaved(what: string): Promise<void> {
    if (this.host && (await this.host.dirty()).length > 0) throw err(`Save your changes before you ${what}.`);
  }

  private requireAuthFor(host: string): void {
    if (this.authRequired.has(host) && !this.credentials.has(host)) throw new BackendError("git-auth", host);
  }

  private files(): GitFile[] {
    this.start();
    const head = this.headTree();
    const work = this.work;
    const out: GitFile[] = [];
    for (const p of new Set([...head.keys(), ...work.keys()])) {
      if (this.conflicts.has(p)) out.push({ path: p, state: "conflicted" });
      else if (!head.has(p)) out.push({ path: p, state: "untracked" });
      else if (!work.has(p)) out.push({ path: p, state: "deleted" });
      else if (!this.content.same(head.get(p), work.get(p))) out.push({ path: p, state: "modified" });
    }
    return out.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  }

  /** Moves the working tree from `from` to `to` (tracked files only; untracked stay). */
  moveWork(from: Tree<S>, to: Tree<S>): void {
    for (const p of from.keys()) if (!to.has(p)) this.writeWork(p, undefined);
    for (const [p, spec] of to) if (!this.content.same(from.get(p), spec)) this.writeWork(p, spec);
  }

  /** Fails if uncommitted changes touch a file that differs between the trees. */
  private guardLocal(from: Tree<S>, to: Tree<S>, what: string): void {
    const touched = this.files()
      .map((f) => f.path)
      .filter((p) => !this.content.same(from.get(p), to.get(p)));
    if (touched.length) throw err(`Commit your changes to ${touched.join(", ")} before you ${what}.`);
  }

  private toCommit(oid: string): GitCommit {
    const c = this.store.get(oid)!;
    return { oid, summary: c.message.split("\n")[0], author: c.author, time: c.time };
  }

  /** The commit `rev` names: an oid (or a prefix of 4+), a local branch, `origin/<branch>`,
   *  `HEAD`, each optionally followed by `~<n>` or `^` (first parent). */
  private resolveRev(rev: string): string {
    this.start();
    this.requireRepo();
    const m = /^(.*?)((?:~\d*|\^)*)$/.exec(rev.trim());
    const name = m?.[1] ?? "";
    let oid: string | null = null;
    if (name === "HEAD") oid = this.head();
    else if (this.branchesMap.has(name)) oid = this.branchesMap.get(name) ?? null;
    else if (name.startsWith("origin/")) oid = this.fetched.get(name.slice("origin/".length)) ?? null;
    else if (this.tagsMap.has(name)) oid = this.tagsMap.get(name)!.oid;
    else if (/^[0-9a-f]{4,40}$/.test(name)) {
      const hits = [...this.store.keys()].filter((o) => o.startsWith(name));
      oid = hits.length === 1 ? hits[0] : null;
    }
    if (!oid) throw err(`Unknown revision "${rev}".`);
    let at: string = oid;
    for (const step of (m?.[2] ?? "").match(/~\d*|\^/g) ?? []) {
      const n = step === "^" ? 1 : Number(step.slice(1) || "1");
      for (let i = 0; i < n; i++) {
        const parent: string | undefined = this.store.get(at)!.parents[0];
        if (!parent) throw err(`Unknown revision "${rev}".`);
        at = parent;
      }
    }
    return at;
  }

  // ---- GitApi ----

  async status(): Promise<GitStatus> {
    this.calls.push("status");
    this.start();
    if (!this.isRepo) {
      return { repo: false, branch: null, upstream: null, ahead: 0, behind: 0, files: [], merging: false };
    }
    const head = this.head();
    const tracked = this.current && this.tracking.has(this.current) ? this.current : null;
    const theirs = tracked ? (this.fetched.get(tracked) ?? null) : null;
    return {
      repo: true,
      branch: head ? this.current : null,
      upstream: tracked ? `origin/${tracked}` : null,
      ahead: head && theirs ? this.countOnly(head, theirs) : 0,
      behind: head && theirs ? this.countOnly(theirs, head) : 0,
      files: this.files(),
      merging: this.mergeHead !== null,
    };
  }

  // Wave 12 (interface): revisions.

  async revParse(rev: string): Promise<string> {
    this.calls.push(`revParse ${rev}`);
    return this.resolveRev(rev);
  }

  async treeAt(rev: string): Promise<GitTree> {
    this.calls.push(`treeAt ${rev}`);
    const oid = this.resolveRev(rev);
    const files: Record<string, string> = {};
    for (const [p, spec] of this.store.get(oid)!.tree) files[p] = this.content.render(spec);
    return { oid, files };
  }

  // Mock helpers (not `GitApi`) for mocks that work on models rather than texts. They
  // return copies.

  /** The file `path` at `rev` as the product models it, `null` if it is not there. */
  fileAt(rev: string, path: string): S | null {
    const spec = this.store.get(this.resolveRev(rev))!.tree.get(path.replace(/^\//, ""));
    return spec ? clone(spec) : null;
  }

  /** The text of the saved file `path` in the working tree (`files.ts`), `null` if there is
   *  none. */
  workText(path: string): string | null {
    this.start();
    const spec = this.work.get(path.replace(/^\//, ""));
    return spec ? this.content.render(spec) : null;
  }

  /** Every file at `rev` as a model, by path. */
  filesAt(rev: string): Map<string, S> {
    return clone(this.store.get(this.resolveRev(rev))!.tree);
  }

  /** The commits of the merge in progress (`null` without one): `HEAD`, the merged commit
   *  and their merge base. */
  /** Wave 14: everything the graph needs, as copies (for `mock-git-graph.ts`). */
  graphData(): {
    commits: { oid: string; parents: string[]; message: string; author: string; time: number }[];
    branches: { name: string; oid: string | null }[];
    remote: { name: string; oid: string }[];
    current: string | null;
    /** Additive (wave 14, strand G): tags and the detached `HEAD`. */
    tags: { name: string; oid: string; message: string | null }[];
    detached: string | null;
  } {
    this.start();
    return {
      commits: [...this.store.values()].map((c) => ({
        oid: c.oid,
        parents: [...c.parents],
        message: c.message,
        author: c.author,
        time: c.time,
      })),
      branches: [...this.branchesMap].map(([name, oid]) => ({ name, oid })),
      remote: [...this.fetched].map(([name, oid]) => ({ name: `origin/${name}`, oid })),
      current: this.current,
      tags: [...this.tagsMap].map(([name, t]) => ({ name, ...t })),
      detached: this.detachedAt,
    };
  }

  // Wave 14 (interface): graph (strand F, `mock-git-graph.ts`) and wizards (strand G,
  // `mock-git-actions.ts`).
  private wave14Graph!: ReturnType<typeof mockGitGraph>;
  private wave14Actions!: ReturnType<typeof mockGitActions>;
  graph: GitApi["graph"] = (q) => this.wave14Graph.graph(q);
  refs: GitApi["refs"] = () => this.wave14Graph.refs();
  changes: GitApi["changes"] = (from, to) => this.wave14Graph.changes(from, to);
  fileDiff: GitApi["fileDiff"] = (from, to, path) => this.wave14Graph.fileDiff(from, to, path);
  remotes: GitApi["remotes"] = () => this.wave14Actions.remotes();
  stashes: GitApi["stashes"] = () => this.wave14Actions.stashes();
  plan: GitApi["plan"] = (action) => this.wave14Actions.plan(action);
  apply: GitApi["apply"] = (action) => this.wave14Actions.apply(action);
  lastAction: GitApi["lastAction"] = () => this.wave14Actions.lastAction();
  undo: GitApi["undo"] = (id) => this.wave14Actions.undo(id);

  // Wave 15 (interface): placeholders until strands B (`mock-git-files.ts`), C
  // (`mock-git-history.ts`), D (`mock-git-plumbing.ts`) and F (`mock-git-index.ts`).
  private wave15Working!: ReturnType<typeof mockGitFiles>;
  private wave15History!: ReturnType<typeof mockGitHistory>;
  private wave15Plumbing!: ReturnType<typeof mockGitPlumbing>;
  private wave15Index!: ReturnType<typeof mockGitIndex>;
  workingCopy: GitApi["workingCopy"] = () => this.wave15Working.workingCopy();
  fileTree: GitApi["fileTree"] = (rev, dir) => this.wave15Working.fileTree(rev, dir);
  blob: GitApi["blob"] = (rev, path) => this.wave15Working.blob(rev, path);
  blame: GitApi["blame"] = (rev, path) => this.wave15History.blame(rev, path);
  searchHistory: GitApi["searchHistory"] = (q) => this.wave15History.searchHistory(q);
  reflog: GitApi["reflog"] = (name, limit) => this.wave15History.reflog(name, limit);
  commitInfo: GitApi["commitInfo"] = (rev) => this.wave15Plumbing.commitInfo(rev);
  pick: GitApi["pick"] = (r) => this.wave15Plumbing.pick(r);
  writeTree: GitApi["writeTree"] = (w) => this.wave15Plumbing.writeTree(w);
  writeCommit: GitApi["writeCommit"] = (w) => this.wave15Plumbing.writeCommit(w);
  resolveText: GitApi["resolveText"] = (path, text) => this.wave15Plumbing.resolveText(path, text);
  indexDiff: GitApi["indexDiff"] = (path, side) => this.wave15Index.indexDiff(path, side);
  stage: GitApi["stage"] = (paths) => this.wave15Index.stage(paths);
  unstage: GitApi["unstage"] = (paths) => this.wave15Index.unstage(paths);
  stagePatch: GitApi["stagePatch"] = (p) => this.wave15Index.stagePatch(p);
  unstagePatch: GitApi["unstagePatch"] = (p) => this.wave15Index.unstagePatch(p);
  discardPatch: GitApi["discardPatch"] = (p) => this.wave15Index.discardPatch(p);
  commitStaged: GitApi["commitStaged"] = (c) => this.wave15Index.commitStaged(c);
  verify: GitApi["verify"] = (oids) => this.wave15Index.verify(oids);
  config: GitApi["config"] = (key) => this.wave15Index.config(key);

  mergeCommits(): { ours: string; theirs: string; base: string | null } | null {
    const ours = this.head();
    if (!this.mergeHead || !ours) return null;
    return { ours, theirs: this.mergeHead, base: this.mergeKind && this.mergeKind !== "merge" ? this.mergeBaseOid : this.baseOf(ours, this.mergeHead) };
  }

  async show(rev: string, path: string): Promise<string | null> {
    this.calls.push(`show ${rev} ${path}`);
    const spec = this.store.get(this.resolveRev(rev))!.tree.get(path.replace(/^\//, ""));
    return spec ? this.content.render(spec) : null;
  }

  async fileLog(path: string, limit: number): Promise<GitFileCommit[]> {
    this.calls.push(`fileLog ${path}`);
    // A leading `/` is relative to the repository (here: the demo repository), as in the shells
    // (wave 15): any file, with the `/` kept in the answer.
    const slash = path.startsWith("/");
    path = path.replace(/^\//, "");
    const head = this.head();
    if (!head) return [];
    const out: GitFileCommit[] = [];
    const ordered = [...this.ancestors(head)].sort((x, y) => this.store.get(y)!.time - this.store.get(x)!.time);
    for (const oid of ordered) {
      if (out.length >= limit) break;
      const c = this.store.get(oid)!;
      const here = c.tree.get(path);
      const before = c.parents[0] ? this.store.get(c.parents[0])!.tree.get(path) : undefined;
      const change = !before && here ? "added" : before && !here ? "deleted" : !this.content.same(before, here) ? "modified" : null;
      if (change) out.push({ commit: this.toCommit(oid), path: slash ? `/${path}` : path, change });
    }
    return out;
  }

  async mergeBase(a: string, b: string): Promise<string | null> {
    this.calls.push(`mergeBase ${a} ${b}`);
    return this.baseOf(this.resolveRev(a), this.resolveRev(b));
  }

  async commits(from: string | null, to: string, limit: number): Promise<GitCommit[]> {
    this.calls.push(`commits ${from ?? ""}..${to}`);
    const hidden = from ? this.ancestors(this.resolveRev(from)) : new Set<string>();
    const out: GitCommit[] = [];
    let oid: string | undefined = this.resolveRev(to);
    while (oid && !hidden.has(oid) && out.length < limit) {
      out.push(this.toCommit(oid));
      oid = this.store.get(oid)!.parents[0];
    }
    return out;
  }

  async diff(path: string): Promise<GitDiff> {
    this.calls.push(`diff ${path}`);
    this.start();
    this.requireRepo();
    const old = this.headTree().get(path);
    const now = this.work.get(path);
    return lineDiff(path, old ? this.content.render(old) : null, now ? this.content.render(now) : null);
  }

  async commit(message: string, paths: string[]): Promise<GitCommit> {
    this.calls.push(`commit ${paths.join(",")}`);
    this.start();
    this.requireRepo();
    if (!this.who) throw err("Set your name and email for commits first.");
    if (!message.trim()) throw err("Write a commit message.");
    if (paths.length === 0) throw err("Select at least one file to commit.");
    if (this.conflicts.size > 0) throw err(`Resolve the conflict in ${[...this.conflicts].join(", ")} first.`);
    const found = this.findings(paths);
    if (found.length > 0) throw new BackendError("secrets-found", mockRefusal(found));
    const head = this.head();
    const tree = clone(this.headTree());
    for (const p of paths) {
      const spec = this.work.get(p);
      if (spec) tree.set(p, clone(spec));
      else tree.delete(p);
    }
    const unchanged =
      tree.size === this.headTree().size && [...tree].every(([p, s]) => this.content.same(s, this.headTree().get(p)));
    if (unchanged && !this.mergeHead) throw err("The selected files have no changes to commit.");
    const twoParents = this.mergeHead && (this.mergeKind ?? "merge") === "merge";
    const parents = [...(head ? [head] : []), ...(twoParents ? [this.mergeHead!] : [])];
    const oid = this.newCommit(parents, tree, message, this.who.name);
    this.branchesMap.set(this.current!, oid);
    this.mergeHead = null;
    this.mergeKind = null;
    this.mergeBaseOid = null;
    this.mergeMessage = null;
    return this.toCommit(oid);
  }

  async log(limit: number): Promise<GitCommit[]> {
    this.calls.push("log");
    this.start();
    const out: GitCommit[] = [];
    for (let o = this.head(); o && out.length < limit; o = this.store.get(o)!.parents[0] ?? null) {
      out.push(this.toCommit(o));
    }
    return out;
  }

  async branches(): Promise<GitBranch[]> {
    this.calls.push("branches");
    this.start();
    if (!this.isRepo) return [];
    const local = [...new Set([...this.branchesMap.keys(), ...(this.current ? [this.current] : [])])].sort();
    return [
      ...local.map((name) => ({ name, current: name === this.current, remote: false })),
      ...[...this.fetched.keys()].sort().map((name) => ({ name: `origin/${name}`, current: false, remote: true })),
    ];
  }

  async checkout(branch: string): Promise<void> {
    this.calls.push(`checkout ${branch}`);
    this.start();
    this.requireRepo();
    await this.requireSaved("switch branches");
    if (this.mergeHead) throw err("Finish the merge (resolve and commit) before you switch branches.");
    let name = branch;
    if (!this.branchesMap.has(branch)) {
      const remote = branch.startsWith("origin/") ? branch.slice("origin/".length) : null;
      const oid = remote ? this.fetched.get(remote) : undefined;
      if (!remote || !oid) throw err(`There is no branch ${branch}.`);
      name = remote;
      if (!this.branchesMap.has(name)) {
        this.branchesMap.set(name, oid);
        this.tracking.add(name);
      }
    }
    const target = this.branchesMap.get(name)!;
    const to = target ? this.store.get(target)!.tree : new Map();
    this.guardLocal(this.headTree(), to, "switch branches");
    this.moveWork(this.headTree(), to);
    this.current = name;
    this.detachedAt = null;
  }

  async createBranch(name: string, checkout: boolean): Promise<void> {
    this.calls.push(`createBranch ${name} ${checkout}`);
    this.start();
    this.requireRepo();
    const n = name.trim();
    if (!/^[A-Za-z0-9][A-Za-z0-9._/-]*$/.test(n) || n.includes("..") || n.endsWith("/")) {
      throw err(`"${name}" is not a valid branch name.`);
    }
    if (this.branchesMap.has(n)) throw err(`A branch named ${n} already exists.`);
    const head = this.head();
    if (!head && !checkout) throw err("Commit something first: a new repository has no commit to branch from.");
    if (head) this.branchesMap.set(n, head);
    if (checkout) {
      this.current = n;
      this.detachedAt = null;
    }
  }

  async push(): Promise<void> {
    this.calls.push("push");
    this.start();
    this.requireRepo();
    if (!this.remote) throw err("There is no remote named origin. Add one on the command line (git remote add).");
    this.requireAuthFor(MOCK_REMOTE_HOST);
    if (this.mergeHead) throw err("Finish the merge (resolve and commit) before you push.");
    const head = this.head();
    if (!head || !this.current) throw err("Commit on a branch before you push.");
    const theirs = this.remote.get(this.current);
    if (theirs && !this.ancestors(head).has(theirs)) {
      throw err("The remote has commits that are not here yet. Pull first, then push again.");
    }
    this.remote.set(this.current, head);
    this.fetched.set(this.current, head);
    this.tracking.add(this.current);
  }

  async pull(): Promise<GitPullResult> {
    this.calls.push("pull");
    this.start();
    this.requireRepo();
    await this.requireSaved("pull");
    if (!this.remote) throw err("There is no remote named origin. Add one on the command line (git remote add).");
    if (this.mergeHead) throw err("Finish the merge (resolve and commit) before you pull.");
    this.requireAuthFor(MOCK_REMOTE_HOST);
    const branch = this.current;
    if (!branch) throw err("Switch to a branch before you pull.");
    const theirs = this.remote.get(branch);
    if (!theirs) throw err(`The remote has no branch ${branch}.`);
    this.fetched.set(branch, theirs);
    this.tracking.add(branch);
    const ours = this.head();
    if (ours === theirs || (ours && this.ancestors(ours).has(theirs))) return { outcome: "up-to-date" };
    const theirsTree = this.store.get(theirs)!.tree;
    if (!ours || this.ancestors(theirs).has(ours)) {
      this.guardLocal(this.headTree(), theirsTree, "pull");
      const commits = this.countOnly(theirs, ours);
      this.moveWork(this.headTree(), theirsTree);
      this.branchesMap.set(branch, theirs);
      return { outcome: "fast-forward", commits };
    }
    if (!this.who) throw err("Set your name and email for commits first: this pull creates a merge commit.");
    const baseOid = this.baseOf(ours, theirs);
    const baseTree = baseOid ? this.store.get(baseOid)!.tree : new Map<string, S>();
    const oursTree = this.headTree();
    this.guardLocal(baseTree, theirsTree, "pull");
    const merged: Tree<S> = new Map();
    const conflicts: string[] = [];
    for (const p of new Set([...baseTree.keys(), ...oursTree.keys(), ...theirsTree.keys()])) {
      const [b, o, t] = [baseTree.get(p), oursTree.get(p), theirsTree.get(p)];
      if (this.content.same(o, t) || this.content.same(b, t)) {
        if (o) merged.set(p, clone(o));
      } else if (this.content.same(b, o)) {
        if (t) merged.set(p, clone(t));
      } else {
        conflicts.push(p);
        if (o) merged.set(p, clone(o));
      }
    }
    this.moveWork(oursTree, merged);
    if (conflicts.length > 0) {
      this.mergeHead = theirs;
      for (const p of conflicts) this.conflicts.add(p);
      return { outcome: "conflicts", paths: conflicts.sort() };
    }
    const oid = this.newCommit([ours, theirs], merged, `Merge origin/${branch} into ${branch}`, this.who.name);
    this.branchesMap.set(branch, oid);
    return { outcome: "merged", oid };
  }

  async conflict(path: string): Promise<GitConflict> {
    this.calls.push(`conflict ${path}`);
    this.start();
    if (!this.mergeHead) throw err("No merge is in progress.");
    const ours = this.head()!;
    const b = this.mergeKind && this.mergeKind !== "merge" ? this.mergeBaseOid : this.baseOf(ours, this.mergeHead);
    const text = (oid: string | null) => {
      const spec = oid ? this.store.get(oid)!.tree.get(path) : undefined;
      return spec ? this.content.render(spec) : null;
    };
    return { path, base: text(b), ours: text(ours), theirs: text(this.mergeHead) };
  }

  async resolve(path: string, resolution: GitResolution): Promise<void> {
    this.calls.push(`resolve ${path} ${resolution}`);
    this.start();
    if (!this.mergeHead) throw err("No merge is in progress.");
    await this.requireSaved("resolve a conflict");
    const side = resolution === "ours" ? this.head()! : this.mergeHead;
    this.writeWork(path, this.store.get(side)!.tree.get(path));
    this.conflicts.delete(path);
  }

  async init(): Promise<void> {
    this.calls.push("init");
    this.start();
    if (this.isRepo) throw err(`The ${this.fixtures.scope} is already a Git repository.`);
    this.isRepo = true;
    this.current = "main";
    this.branchesMap.clear();
    this.remote = null;
    this.remoteUrls.clear();
  }

  /** What the "Choose folder…" picker returns; `undefined` removes the picker (browser-like). */
  pickRepository: (() => Promise<string | null>) | undefined = async () => {
    this.calls.push("pickRepository");
    return this.picked;
  };

  /** The folder the mock picker "chooses" (`null`: cancelled). */
  picked: string | null = "/Users/demo/origin/api-tests.git";

  async clone(url: string): Promise<GitOwnerInfo | null> {
    this.calls.push(`clone ${url}`);
    this.start();
    if (looksLocal(url)) {
      // Like the desktop shell: any path is accepted. Without `pickRepository` the mock
      // behaves like the browser, which cannot read paths.
      if (!this.pickRepository) throw err(LOCAL_CLONE_UNSUPPORTED);
    } else {
      let host: string;
      try {
        host = new URL(url.trim()).host;
      } catch {
        throw err("Enter an HTTPS URL of the repository (SSH needs the desktop app).");
      }
      this.requireAuthFor(host);
    }
    this.scenario("clean");
    return this.host ? this.host.openOwner() : { id: "mock-clone", name: "Clone", location: url };
  }

  async identity(): Promise<GitIdentity | null> {
    this.start();
    return this.who ? { ...this.who } : null;
  }

  async setIdentity(identity: GitIdentity): Promise<void> {
    this.calls.push("setIdentity");
    this.start();
    const name = identity.name.trim();
    const email = identity.email.trim();
    if (!name) throw err("Enter your name for commits.");
    if (!/^[^\s@<>]+@[^\s@<>]+$/.test(email)) throw err("Enter a valid email address for commits.");
    this.who = { name, email };
  }

  async hasCredentials(host: string): Promise<boolean> {
    this.start();
    return this.credentials.has(host);
  }

  async setCredentials(host: string, username: string, password: string): Promise<void> {
    this.calls.push(`setCredentials ${host}`);
    this.start();
    this.credentials.set(host, { username, password });
  }

  async deleteCredentials(host: string): Promise<void> {
    this.credentials.delete(host);
  }

  /** Wave 5, strand E: the mock's secret scan (`mock-secret-scan.ts`) of the working tree. */
  async scan(paths: string[]): Promise<SecretFinding[]> {
    this.calls.push(`scan ${paths.join(",")}`);
    this.start();
    return this.findings(paths);
  }

  private findings(paths: string[]): SecretFinding[] {
    const files: [string, string][] = [];
    for (const p of paths) {
      const spec = this.work.get(p);
      if (spec) files.push([p, this.content.render(spec)]);
    }
    return mockScan(files);
  }
}

/** A mock Git over plain text files, with the default fixtures. */
export function mockGit(options: MockGitOptions = {}): MockGit {
  return new MockGit(options);
}

/** What a scenario of the fixtures may read and change: see `Core` (./actions.ts). */
export type MockScenarioCore<S> = Core<S>;

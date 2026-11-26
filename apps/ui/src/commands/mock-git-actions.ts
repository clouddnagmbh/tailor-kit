// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand G: the wizards' actions on the mock's Git (`GitApi.plan`, `apply`, `undo`,
// `remotes`, `stashes`, `lastAction`), with the rules every shell follows (see
// `rt_app::git::actions`): plan only reads; apply plans again and refuses with kind `git` when
// blocked; whatever moves a ref or changes files writes a backup ref and a journal entry first;
// undo puts refs and files back and refuses once the refs moved since.
//
// The state lives in `MockGit` (mock-git.ts, also strand G's); this module reads and writes its
// fields through `Core` (the same object, typed with the members it uses). Merges, cherry-picks
// and reverts merge request files field by field, as the real semantic merge does for
// collection files.

import { BackendError } from "./backend";
import type { MockGit } from "./mock-git";
import type {
  GitAction,
  GitActionResult,
  GitBlock,
  GitBlockReason,
  GitCommit,
  GitFile,
  GitMergeFile,
  GitPlan,
  GitPullResult,
  GitRefMove,
  GitRemote,
  GitStash,
  GitUndoEntry,
  RequestSpec,
} from "./types";
import type { GitActionMethods } from "./wave14-stubs";
import { refNameProblem } from "@tailor-kit/git-ui";

type Tree = Map<string, RequestSpec>;

/** A commit of the mock (as in mock-git.ts). */
interface Commit {
  oid: string;
  parents: string[];
  tree: Tree;
  message: string;
  author: string;
  time: number;
}

/** A parked change: a commit of the whole working tree on top of `HEAD`. */
export interface MockStash {
  oid: string;
  message: string;
  time: number;
  branch: string | null;
  files: string[];
}

/** Everything undo needs: the state before the action. */
interface Snapshot {
  branches: Map<string, string | null>;
  current: string | null;
  detachedAt: string | null;
  tags: Map<string, { oid: string; message: string | null }>;
  tracking: Set<string>;
  remoteUrls: Map<string, string>;
  hadRemote: boolean;
  stashes: MockStash[];
  work: Tree;
  mergeHead: string | null;
  conflicts: Set<string>;
}

export interface MockJournalEntry {
  entry: GitUndoEntry;
  before: Snapshot;
}

/** The members of `MockGit` this module uses (mock-git.ts declares them). */
export interface Core {
  store: Map<string, Commit>;
  branchesMap: Map<string, string | null>;
  current: string | null;
  detachedAt: string | null;
  tagsMap: Map<string, { oid: string; message: string | null }>;
  tracking: Set<string>;
  fetched: Map<string, string>;
  remote: Map<string, string> | null;
  remoteUrls: Map<string, string>;
  remoteTags: Set<string>;
  mergeHead: string | null;
  /** What the merge in progress is: a merge (two parents) or a pick/revert (one). */
  mergeKind: "merge" | "cherry-pick" | "revert" | null;
  /** The base of a pick/revert in progress. */
  mergeBaseOid: string | null;
  mergeMessage: string | null;
  conflicts: Set<string>;
  stashList: MockStash[];
  journal: MockJournalEntry[];
  backups: Map<string, string>;
  calls: string[];
  work: Tree;
  isRepo: boolean;
  head(): string | null;
  headTree(): Tree;
  files(): GitFile[];
  resolveRev(rev: string): string;
  newCommit(parents: string[], tree: Tree, message: string, author: string): string;
  ancestors(oid: string): Set<string>;
  baseOf(a: string, b: string): string | null;
  moveWork(from: Tree, to: Tree): void;
  writeWork(path: string, spec: RequestSpec | undefined): void;
  toCommit(oid: string): GitCommit;
  requireAuthFor(host: string): void;
  identity(): Promise<{ name: string; email: string } | null>;
  pull(): Promise<GitPullResult>;
  push(): Promise<void>;
  start(): void;
  dirtyFiles(): Promise<string[]>;
}

const clone = <T>(v: T): T => structuredClone(v);
const json = (v: unknown) => JSON.stringify(v ?? null);
const same = (a: RequestSpec | undefined, b: RequestSpec | undefined) => json(a) === json(b);
const short = (oid: string) => oid.slice(0, 7);
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const slash = (p: string) => (p.startsWith("/") ? p : `/${p}`);
const unslash = (p: string) => p.replace(/^\//, "");
const subjectOf = (c: Commit) => c.message.split("\n")[0];

/** Selects repository paths: a file, or a folder with everything under it; no paths: all. */
function selection(paths: string[]): (p: string) => boolean {
  const roots = paths.map((x) => unslash(x).replace(/\/+$/, "")).filter((x) => x !== "");
  return (p) => roots.length === 0 || roots.some((r) => p === r || p.startsWith(`${r}/`));
}

/** One file of a three-way merge, field by field for requests. */
interface FileMerge {
  path: string;
  merged: RequestSpec | undefined;
  clean: number;
  conflicts: string[];
}

function mergeFile(path: string, b: RequestSpec | undefined, o: RequestSpec | undefined, t: RequestSpec | undefined): FileMerge | null {
  if (same(o, t) || same(b, t)) return null;
  if (same(b, o)) return { path, merged: t, clean: 1, conflicts: [] };
  if (!o || !t || !b) {
    return { path, merged: o, clean: 0, conflicts: [!o ? "deleted here, changed there" : !t ? "changed here, deleted there" : "added on both sides"] };
  }
  const merged: Record<string, unknown> = { ...o };
  let clean = 0;
  const conflicts: string[] = [];
  const bo = b as unknown as Record<string, unknown>;
  const oo = o as unknown as Record<string, unknown>;
  const to = t as unknown as Record<string, unknown>;
  for (const key of new Set([...Object.keys(bo), ...Object.keys(oo), ...Object.keys(to)])) {
    const [bv, ov, tv] = [json(bo[key]), json(oo[key]), json(to[key])];
    if (ov === tv || bv === tv) continue;
    if (bv === ov) {
      merged[key] = clone(to[key]);
      clean++;
    } else conflicts.push(key);
  }
  return { path, merged: conflicts.length ? o : (merged as unknown as RequestSpec), clean, conflicts };
}

/** Exported for the tests: the field-by-field merge of whole trees. */
export function mergeTrees(base: Tree, ours: Tree, theirs: Tree): FileMerge[] {
  const out: FileMerge[] = [];
  for (const p of [...new Set([...base.keys(), ...ours.keys(), ...theirs.keys()])].sort()) {
    const m = mergeFile(p, base.get(p), ours.get(p), theirs.get(p));
    if (m) out.push(m);
  }
  return out;
}

const preview = (files: FileMerge[]): GitMergeFile[] =>
  files.map((f) => ({ path: slash(f.path), semantic: f.path.endsWith(".yml"), clean: f.clean, conflicts: f.conflicts }));

function diffPaths(a: Tree, b: Tree): string[] {
  return [...new Set([...a.keys(), ...b.keys()])]
    .filter((x) => !same(a.get(x), b.get(x)))
    .sort()
    .map(slash);
}

export interface MockGitActions extends GitActionMethods {
  /** Test helper: the backup refs (name → commit). */
  backupRefs(): Map<string, string>;
}

export function mockGitActions(git: MockGit): MockGitActions {
  const c = git as unknown as Core;
  const now = () => Math.floor(Date.now() / 1000);
  let seq = 0;

  const block = (reason: GitBlockReason, message: string): GitBlock => ({ reason, message });

  async function unsaved(): Promise<GitBlock | null> {
    return (await c.dirtyFiles()).length > 0 ? block("unsaved-changes", "Save your changes in the app first.") : null;
  }

  const inProgress = (): GitBlock | null =>
    c.mergeHead || c.conflicts.size > 0
      ? block(
          "in-progress",
          c.conflicts.size > 0
            ? "A merge is waiting for your decision: resolve the conflicts and commit first."
            : "Commit to finish the merge in progress first.",
        )
      : null;

  const localChanges = (what: string): GitBlock | null => {
    const files = c.files();
    return files.length > 0
      ? block("local-changes", `You have ${plural(files.length, "uncommitted change")}. Commit or park them before you ${what}.`)
      : null;
  };

  /** Files with uncommitted changes that differ between `from` and `to` (would be overwritten). */
  const touched = (from: Tree, to: Tree) =>
    c
      .files()
      .map((f) => f.path)
      .filter((p) => !same(from.get(p), to.get(p)));

  const resolve = (rev: string): string | null => {
    try {
      return c.resolveRev(rev);
    } catch {
      return null;
    }
  };

  const branchRef = (name: string) => `refs/heads/${name}`;
  const snapshot = (): Snapshot => ({
    branches: new Map(c.branchesMap),
    current: c.current,
    detachedAt: c.detachedAt,
    tags: clone(c.tagsMap),
    tracking: new Set(c.tracking),
    remoteUrls: new Map(c.remoteUrls),
    hadRemote: c.remote !== null,
    stashes: clone(c.stashList),
    work: clone(c.work),
    mergeHead: c.mergeHead,
    conflicts: new Set(c.conflicts),
  });

  /** The current value of a ref by full name (as the journal records them). */
  function refValue(name: string): string | null {
    if (name === "HEAD") return c.head();
    if (name.startsWith("refs/heads/")) return c.branchesMap.get(name.slice(11)) ?? null;
    if (name.startsWith("refs/tags/")) return c.tagsMap.get(name.slice(10))?.oid ?? null;
    if (name === "refs/stash") return c.stashList[0]?.oid ?? null;
    return null;
  }

  /** Commits reachable from `oid` and from no other branch, remote branch or tag. */
  function onlyOn(oid: string, except: string): number {
    const others = new Set<string>();
    for (const [n, o] of c.branchesMap) if (n !== except && o) for (const a of c.ancestors(o)) others.add(a);
    for (const o of c.fetched.values()) for (const a of c.ancestors(o)) others.add(a);
    for (const t of c.tagsMap.values()) for (const a of c.ancestors(t.oid)) others.add(a);
    return [...c.ancestors(oid)].filter((o) => !others.has(o)).length;
  }

  function commitsBetween(from: string, notIn: string | null, limit = 50): GitCommit[] {
    const hidden = notIn ? c.ancestors(notIn) : new Set<string>();
    return [...c.ancestors(from)]
      .filter((o) => !hidden.has(o))
      .sort((a, b) => c.store.get(b)!.time - c.store.get(a)!.time)
      .slice(0, limit)
      .map((o) => c.toCommit(o));
  }

  const host = (remote: string) => {
    try {
      return new URL(c.remoteUrls.get(remote) ?? "").host;
    } catch {
      return "";
    }
  };

  // ---------------------------------------------------------------- plan

  async function plan(action: GitAction): Promise<GitPlan> {
    c.start();
    const p: GitPlan = { action, steps: [], refs: [], network: false, backup: false, undoable: true, destructive: false };
    if (!c.isRepo) return { ...p, undoable: false, blocked: block("not-found", "This folder is not a Git repository yet.") };
    const head = c.head();
    const cur = c.current && head ? c.current : null;
    const stop = (b: GitBlock | null) => {
      if (b && !p.blocked) p.blocked = b;
    };
    const needBranch = () =>
      stop(cur ? null : block("detached", c.current ? "Make the first commit first." : "You are not on a branch. Create a branch here or switch to one first."));

    switch (action.action) {
      case "create-branch": {
        const problem = refNameProblem(action.name);
        const start = resolve(action.start);
        p.refs.push({ name: branchRef(action.name), before: null, after: start });
        p.steps.push(`Creates the branch ${action.name} at ${start ? `${short(start)} ("${subjectOf(c.store.get(start)!)}")` : action.start}.`);
        stop(problem ? block("invalid-name", problem) : null);
        stop(c.branchesMap.has(action.name) ? block("exists", `A branch named ${action.name} exists already. Choose another name.`) : null);
        stop(start ? null : block("not-found", `There is no commit or branch "${action.start}".`));
        if (action.switch) {
          p.steps.push(`Switches to ${action.name}; new commits go there.`);
          p.refs.push({ name: "HEAD", before: head, after: start });
          if (start && start !== head) {
            p.backup = true;
            stop(await unsaved());
            stop(inProgress());
            const t = touched(c.headTree(), c.store.get(start)!.tree);
            stop(t.length ? block("local-changes", `Your changes to ${t.join(", ")} would be overwritten. Commit or park them first.`) : null);
            p.files = diffPaths(c.headTree(), c.store.get(start)!.tree);
          }
        }
        return p;
      }
      case "switch": {
        const t = action.target;
        const local = c.branchesMap.has(t);
        const remote = !local && t.startsWith("origin/") && c.fetched.has(t.slice(7));
        const target = resolve(t);
        if (!target) return { ...p, blocked: block("not-found", `There is no branch, tag or commit "${t}".`) };
        if (local && t === c.current) return { ...p, undoable: false, blocked: block("nothing-to-do", `You are on ${t} already.`) };
        if (remote) {
          const name = t.slice(7);
          if (c.branchesMap.has(name)) p.steps.push(`Switches to your branch ${name} (it follows ${t}).`);
          else {
            p.steps.push(`Creates the branch ${name} from ${t}, linked to it, and switches to it.`);
            p.refs.push({ name: branchRef(name), before: null, after: target });
          }
        } else if (local) {
          p.steps.push(`Switches to ${t}; your files show its state.`);
        } else {
          p.steps.push(`Shows the files of ${t} (${short(target)}). You will not be on a branch: create one there to keep new commits.`);
          p.warnings = [`${t} is not a branch.`];
        }
        p.refs.push({ name: "HEAD", before: head, after: target });
        p.files = diffPaths(c.headTree(), c.store.get(target)!.tree);
        p.backup = true;
        stop(await unsaved());
        stop(inProgress());
        const tt = touched(c.headTree(), c.store.get(target)!.tree);
        stop(tt.length ? block("local-changes", `Your changes to ${tt.join(", ")} would be overwritten. Commit or park them first.`) : null);
        return p;
      }
      case "rename-branch": {
        const at = c.branchesMap.get(action.from);
        p.steps.push(`Renames ${action.from} to ${action.to}.`);
        p.refs.push({ name: branchRef(action.from), before: at ?? null, after: null }, { name: branchRef(action.to), before: null, after: at ?? null });
        if (c.tracking.has(action.from)) p.warnings = [`The server keeps ${action.from}; publish ${action.to} to put the new name there.`];
        const problem = refNameProblem(action.to);
        stop(c.branchesMap.has(action.from) ? null : block("not-found", `There is no branch ${action.from}.`));
        stop(problem ? block("invalid-name", problem) : null);
        stop(c.branchesMap.has(action.to) ? block("exists", `A branch named ${action.to} exists already.`) : null);
        return p;
      }
      case "delete-branch": {
        const at = c.branchesMap.get(action.name);
        if (at === undefined) return { ...p, blocked: block("not-found", `There is no branch ${action.name}.`) };
        if (action.name === c.current) {
          return { ...p, blocked: block("current-branch", `You are on ${action.name}. Switch to another branch before you delete it.`) };
        }
        const only = at ? onlyOn(at, action.name) : 0;
        p.steps.push(`Deletes the branch ${action.name} on this computer${c.tracking.has(action.name) ? " (the server keeps its copy)" : ""}.`);
        p.refs.push({ name: branchRef(action.name), before: at, after: null });
        p.backup = true;
        if (only > 0) {
          p.destructive = true;
          p.warnings = [`${plural(only, "commit is", "commits are")} only on ${action.name}. The backup keeps ${only === 1 ? "it" : "them"} for Undo.`];
          if (!action.force) {
            p.blocked = block(
              "not-merged",
              `${action.name} has ${plural(only, "commit")} that ${only === 1 ? "is" : "are"} on no other branch. Confirm to delete it anyway.`,
            );
          }
        }
        return p;
      }
      case "merge": {
        needBranch();
        const from = resolve(action.from);
        if (!from) return { ...p, blocked: block("not-found", `There is no branch, tag or commit "${action.from}".`) };
        if (!head || !cur) return p;
        if (c.ancestors(head).has(from)) {
          return { ...p, undoable: false, blocked: block("nothing-to-do", `${cur} already contains everything from ${action.from}.`) };
        }
        const ff = c.ancestors(from).has(head);
        const incoming = commitsBetween(from, head);
        const baseTree = c.store.get(c.baseOf(head, from) ?? "")?.tree ?? new Map<string, RequestSpec>();
        const files = ff ? [] : mergeTrees(baseTree, c.headTree(), c.store.get(from)!.tree);
        const ffFiles = diffPaths(c.headTree(), c.store.get(from)!.tree);
        p.merge = {
          fast_forward: ff,
          commits: incoming,
          files: ff ? ffFiles.map((path) => ({ path, semantic: path.endsWith(".yml"), clean: 1, conflicts: [] })) : preview(files),
        };
        const conflicts = files.filter((f) => f.conflicts.length > 0).length;
        p.steps.push(
          ff
            ? `Moves ${cur} forward to ${action.from}: ${plural(incoming.length, "commit")} come in, no merge commit needed.`
            : `Brings ${plural(incoming.length, "commit")} from ${action.from} into ${cur} with a merge commit.`,
        );
        if (conflicts > 0) p.steps.push(`${plural(conflicts, "file needs", "files need")} your decision afterwards: the same fields were changed on both sides.`);
        p.refs.push({ name: branchRef(cur), before: head, after: ff ? from : "" });
        p.files = ff ? ffFiles : files.map((f) => slash(f.path));
        p.backup = true;
        stop(await unsaved());
        stop(inProgress());
        stop(localChanges("merge"));
        return p;
      }
      case "pull": {
        needBranch();
        p.network = true;
        p.backup = true;
        const up = cur && c.tracking.has(cur) ? `origin/${cur}` : null;
        if (cur && !up) stop(block("no-upstream", `${cur} is not on the server yet. Publish it first.`));
        p.steps.push(`Gets new commits from ${up ?? "the server"} and merges them into ${cur ?? "your branch"}.`);
        if (cur && head) p.refs.push({ name: branchRef(cur), before: head, after: "" });
        stop(await unsaved());
        stop(inProgress());
        return p;
      }
      case "push": {
        needBranch();
        p.network = true;
        p.undoable = false;
        const up = cur && c.tracking.has(cur) ? cur : null;
        if (cur && !up) stop(block("no-upstream", `${cur} is not on the server yet. Publish it first.`));
        if (cur && up && head) {
          const theirs = c.fetched.get(up) ?? null;
          const mine = c.ancestors(head);
          const ahead = theirs ? [...mine].filter((o) => !c.ancestors(theirs).has(o)).length : mine.size;
          const behind = theirs ? [...c.ancestors(theirs)].filter((o) => !mine.has(o)).length : 0;
          p.steps.push(`Sends ${plural(ahead, "commit")} to origin/${up}. Sending cannot be undone from the app.`);
          p.refs.push({ name: `refs/remotes/origin/${up}`, before: theirs, after: head });
          if (behind > 0 && ahead > 0) stop(block("diverged", `origin/${up} has ${plural(behind, "commit")} you do not have. Bring both together first.`));
          else if (behind > 0) stop(block("rejected", `origin/${up} has ${plural(behind, "new commit")}. Pull first, then push.`));
          else if (ahead === 0) stop(block("nothing-to-do", "Nothing to send: the server has all your commits."));
        }
        stop(inProgress());
        return p;
      }
      case "publish": {
        p.network = true;
        p.undoable = false;
        const at = c.branchesMap.get(action.branch);
        stop(at === undefined ? block("not-found", `There is no branch ${action.branch}.`) : null);
        stop(at === null ? block("nothing-to-do", "Make the first commit first.") : null);
        stop(c.remoteUrls.has(action.remote) ? null : block("not-found", `There is no remote ${action.remote}. Add it first.`));
        stop(c.tracking.has(action.branch) ? block("nothing-to-do", `${action.branch} is on the server already.`) : null);
        p.steps.push(`Puts ${action.branch} on ${action.remote} and links the two, so Pull and Push work from now on.`);
        p.refs.push({ name: `refs/remotes/${action.remote}/${action.branch}`, before: null, after: at ?? null });
        return p;
      }
      case "fetch": {
        p.network = true;
        p.undoable = false;
        const name = action.remote ?? "origin";
        stop(c.remoteUrls.has(name) ? null : block("not-found", `There is no remote ${name}.`));
        p.steps.push(`Asks ${name} what is new. Your branches and files stay as they are.`);
        return p;
      }
      case "add-remote": {
        const problem = refNameProblem(action.name);
        stop(problem ? block("invalid-name", problem) : null);
        stop(c.remoteUrls.has(action.name) ? block("exists", `A remote named ${action.name} exists already.`) : null);
        stop(action.url.trim() ? null : block("invalid-name", "Enter the address of the repository on the server."));
        p.steps.push(`Links this repository to ${action.url.trim() || "the server"} as "${action.name}".`);
        return p;
      }
      case "remove-remote": {
        stop(c.remoteUrls.has(action.name) ? null : block("not-found", `There is no remote ${action.name}.`));
        p.steps.push(`Removes the link to ${action.name}. Nothing on the server changes.`);
        return p;
      }
      case "rename-remote": {
        const problem = refNameProblem(action.to);
        stop(c.remoteUrls.has(action.from) ? null : block("not-found", `There is no remote ${action.from}.`));
        stop(problem ? block("invalid-name", problem) : null);
        stop(c.remoteUrls.has(action.to) ? block("exists", `A remote named ${action.to} exists already.`) : null);
        p.steps.push(`Renames the remote ${action.from} to ${action.to}.`);
        return p;
      }
      case "set-remote-url": {
        stop(c.remoteUrls.has(action.name) ? null : block("not-found", `There is no remote ${action.name}.`));
        stop(action.url.trim() ? null : block("invalid-name", "Enter the address of the repository on the server."));
        p.steps.push(`Changes the address of ${action.name} to ${action.url.trim()}.`);
        return p;
      }
      case "stash": {
        const paths = action.paths ?? [];
        const sel = selection(paths);
        const files = c.files().filter((f) => sel(f.path));
        stop(files.length === 0 ? block("nothing-to-do", paths.length === 0 ? "There are no changes to park." : "None of these files has changes to park.") : null);
        stop(await unsaved());
        stop(inProgress());
        p.steps.push(
          paths.length === 0
            ? `Parks ${plural(files.length, "change")} and makes your files match the last commit. Bring them back any time.`
            : `Parks ${plural(files.length, "change")} and makes these files match the last commit; your other changes stay. Bring them back any time.`,
        );
        p.refs.push({ name: "refs/stash", before: c.stashList[0]?.oid ?? null, after: "" });
        p.files = files.map((f) => slash(f.path));
        return p;
      }
      case "unstash": {
        const s = c.stashList[action.index];
        if (!s) return { ...p, blocked: block("not-found", "There are no such parked changes.") };
        const tree = c.store.get(s.oid)!.tree;
        p.steps.push(`Brings back "${s.message}" (${plural(s.files.length, "file")}).`);
        if (!action.keep) {
          p.steps.push("Removes it from the parked changes; a backup keeps it for Undo.");
          p.refs.push({ name: "refs/stash", before: c.stashList[0].oid, after: action.index === 0 ? (c.stashList[1]?.oid ?? null) : c.stashList[0].oid });
        }
        p.backup = true;
        p.files = s.files.map(slash);
        stop(await unsaved());
        stop(inProgress());
        const clash = c
          .files()
          .map((f) => f.path)
          .filter((x) => s.files.includes(x) && !same(c.work.get(x), tree.get(x)));
        stop(clash.length ? block("local-changes", `Your changes to ${clash.join(", ")} would be overwritten. Commit or park them first.`) : null);
        return p;
      }
      case "drop-stash": {
        const s = c.stashList[action.index];
        if (!s) return { ...p, blocked: block("not-found", "There are no such parked changes.") };
        p.steps.push(`Deletes the parked changes "${s.message}".`);
        p.refs.push({ name: "refs/stash", before: c.stashList[0].oid, after: action.index === 0 ? (c.stashList[1]?.oid ?? null) : c.stashList[0].oid });
        p.backup = true;
        p.destructive = true;
        return p;
      }
      case "revert":
      case "cherry-pick": {
        needBranch();
        const oid = resolve(action.commit);
        if (!oid) return { ...p, blocked: block("not-found", `There is no commit "${action.commit}".`) };
        if (!head || !cur) return p;
        const commit = c.store.get(oid)!;
        if (commit.parents.length > 1) {
          return { ...p, blocked: block("unsupported", "This is a merge commit. Undoing or copying merges works on the command line for now.") };
        }
        if (commit.parents.length === 0 && action.action === "revert") {
          return { ...p, blocked: block("unsupported", "This is the very first commit; undoing it would remove everything.") };
        }
        const pick = action.action === "cherry-pick";
        if (pick && c.ancestors(head).has(oid)) return { ...p, undoable: false, blocked: block("nothing-to-do", `${cur} contains this commit already.`) };
        const parent = commit.parents[0] ? c.store.get(commit.parents[0])!.tree : new Map<string, RequestSpec>();
        const files = pick ? mergeTrees(parent, c.headTree(), commit.tree) : mergeTrees(commit.tree, c.headTree(), parent);
        const subject = subjectOf(commit);
        p.steps.push(
          pick
            ? `Copies the changes of "${subject}" onto ${cur} as a new commit.`
            : `Makes a new commit on ${cur} that takes back "${subject}". History stays as it is.`,
        );
        const conflicts = files.filter((f) => f.conflicts.length > 0).length;
        if (conflicts > 0) p.steps.push(`${plural(conflicts, "file needs", "files need")} your decision afterwards.`);
        if (files.length === 0) stop(block("nothing-to-do", pick ? "These changes are on your branch already." : "Your branch no longer has these changes."));
        p.merge = { fast_forward: false, commits: [c.toCommit(oid)], files: preview(files) };
        p.refs.push({ name: branchRef(cur), before: head, after: "" });
        p.files = files.map((f) => slash(f.path));
        p.backup = true;
        stop(await unsaved());
        stop(inProgress());
        stop(localChanges(pick ? "copy a commit" : "undo a commit"));
        return p;
      }
      case "reset": {
        needBranch();
        const target = resolve(action.target);
        if (!target) return { ...p, blocked: block("not-found", `There is no commit "${action.target}".`) };
        if (!head || !cur) return p;
        if (target === head && action.mode === "keep-changes") return { ...p, undoable: false, blocked: block("nothing-to-do", `${cur} is there already.`) };
        const leaving = [...c.ancestors(head)].filter((o) => !c.ancestors(target).has(o)).length;
        p.steps.push(`Moves ${cur} to ${short(target)} ("${subjectOf(c.store.get(target)!)}").`);
        p.steps.push(
          action.mode === "keep-changes"
            ? "Your files stay as they are; what the removed commits did shows as changes."
            : "Your files match that commit; uncommitted changes are thrown away (the backup keeps them).",
        );
        const warnings: string[] = [];
        if (leaving > 0) warnings.push(`${plural(leaving, "commit leaves", "commits leave")} ${cur}. The backup keeps ${leaving === 1 ? "it" : "them"} for Undo.`);
        const theirs = c.tracking.has(cur) ? c.fetched.get(cur) : undefined;
        if (leaving > 0 && theirs && [...c.ancestors(head)].some((o) => !c.ancestors(target).has(o) && c.ancestors(theirs).has(o))) {
          warnings.push("Some of these commits are on the server already. The next Pull brings them back.");
        }
        if (warnings.length) p.warnings = warnings;
        p.refs.push({ name: branchRef(cur), before: head, after: target });
        p.backup = true;
        if (action.mode === "discard") {
          p.destructive = true;
          p.files = diffPaths(c.work, c.store.get(target)!.tree);
          stop(await unsaved());
        }
        stop(inProgress());
        return p;
      }
      case "discard": {
        const all = c.files();
        const wanted = action.paths.length ? action.paths.map(unslash) : all.map((f) => f.path);
        const hit = all.filter((f) => wanted.includes(f.path));
        if (hit.length === 0) return { ...p, undoable: false, blocked: block("nothing-to-do", "There are no changes to discard.") };
        p.steps.push(`Throws away the changes to ${plural(hit.length, "file")}; they go back to the last commit.`);
        p.steps.push("A backup keeps them, so Undo can bring them back.");
        p.files = hit.map((f) => slash(f.path));
        p.backup = true;
        p.destructive = true;
        stop(await unsaved());
        stop(hit.some((f) => f.state === "conflicted") ? block("in-progress", "Resolve the conflicts first.") : null);
        return p;
      }
      case "amend": {
        needBranch();
        if (!head || !cur) return p;
        stop(inProgress());
        const paths = action.paths ?? [];
        if (paths.length > 0) stop(await unsaved());
        const commit = c.store.get(head)!;
        if (action.message !== undefined && !action.message.trim()) return { ...p, blocked: block("invalid-name", "The commit message is empty.") };
        if ([...c.fetched.values()].some((t) => c.ancestors(t).has(head))) {
          return { ...p, blocked: block("published", "The last commit is already on the server, so changing it would rewrite history others may have. Make a new commit instead.") };
        }
        const sel = selection(paths);
        const files = paths.length > 0 ? c.files().filter((f) => f.state !== "conflicted" && sel(f.path)) : [];
        const message = action.message?.trim();
        const newMessage = message && message !== commit.message.trim() ? message : null;
        if (files.length === 0 && !newMessage) {
          return {
            ...p,
            undoable: false,
            blocked: p.blocked ?? block("nothing-to-do", paths.length === 0 ? "There is nothing to change: give a new message or choose files." : "Nothing changed in these files since the last commit."),
          };
        }
        const what: string[] = [];
        if (newMessage) what.push(`the message "${newMessage.split("\n")[0]}"`);
        if (files.length > 0) what.push(`the current content of ${plural(files.length, "file")}`);
        p.steps.push(`Replaces the last commit ${short(head)} ("${subjectOf(commit)}") on ${cur} with a new one that has ${what.join(" and ")}. Its author and place in history stay.`);
        p.steps.push("Your files do not change. The backup keeps the old commit for Undo.");
        p.files = files.map((f) => slash(f.path));
        p.refs.push({ name: branchRef(cur), before: head, after: null });
        p.backup = true;
        return p;
      }
      case "abort": {
        if (!c.mergeHead && c.conflicts.size === 0) return { ...p, undoable: false, blocked: block("nothing-to-do", "Nothing is in progress that could be stopped.") };
        const files = c.files();
        p.steps.push(`Stops the ${c.mergeKind ?? "merge"} and puts your files back the way they were before it started. Your branch does not move.`);
        if (files.length > 0) {
          p.steps.push(`The changes in ${plural(files.length, "file")} are taken out of your working files.`);
          p.warnings = ["Undo brings your files back as plain changes; it cannot resume the operation."];
          p.destructive = true;
        }
        p.files = files.map((f) => slash(f.path));
        p.backup = true;
        stop(await unsaved());
        return p;
      }
      case "restore-files": {
        if (action.paths.length === 0) return { ...p, blocked: block("nothing-to-do", "Choose the files to restore.") };
        const at = resolve(action.rev);
        if (!at) return { ...p, blocked: block("not-found", `There is no commit "${action.rev}".`) };
        const want = c.store.get(at)!.tree;
        const sel = selection(action.paths);
        const hits = [...new Set([...want.keys(), ...c.work.keys()])].filter((x) => sel(x) && !same(want.get(x), c.work.get(x))).sort();
        if (hits.length === 0) return { ...p, undoable: false, blocked: block("nothing-to-do", "These files are the same already.") };
        const deleted = hits.filter((x) => !want.has(x)).length;
        const changed = new Set(c.files().map((f) => f.path));
        const replaced = hits.filter((x) => changed.has(x)).length;
        p.steps.push(`Makes ${plural(hits.length, "file")} the way ${hits.length === 1 ? "it is" : "they are"} ${short(at)} ("${subjectOf(c.store.get(at)!)}").`);
        if (deleted > 0) p.steps.push(`${plural(deleted, "file")} did not exist there and ${deleted === 1 ? "is" : "are"} deleted.`);
        if (replaced > 0) {
          p.warnings = [`${plural(replaced, "file has", "files have")} uncommitted changes. They are replaced; the backup keeps them for Undo.`];
          p.destructive = true;
        }
        p.steps.push("Your branch and its commits do not move.");
        p.files = hits.map(slash);
        p.backup = true;
        stop(await unsaved());
        stop(inProgress());
        return p;
      }
      case "restore-branch": {
        const problem = refNameProblem(action.name);
        if (problem) return { ...p, blocked: block("invalid-name", problem) };
        const to = resolve(action.target);
        if (!to) return { ...p, blocked: block("not-found", `There is no commit "${action.target}".`) };
        if (cur === action.name) return { ...p, blocked: block("current-branch", `${action.name} is the branch you are on. Use Reset to move it.`) };
        const old = c.branchesMap.get(action.name) ?? null;
        if (old === to) return { ...p, undoable: false, blocked: block("nothing-to-do", `${action.name} is there already.`) };
        const named = `${short(to)} ("${subjectOf(c.store.get(to)!)}")`;
        if (old === null) p.steps.push(`Creates the branch ${action.name} again at ${named}.`);
        else {
          p.steps.push(`Moves ${action.name} to ${named}.`);
          const leaving = [...c.ancestors(old)].filter((o) => !c.ancestors(to).has(o)).length;
          if (leaving > 0) p.warnings = [`${plural(leaving, "commit leaves", "commits leave")} ${action.name}. The backup keeps ${leaving === 1 ? "it" : "them"} for Undo.`];
          p.backup = true;
        }
        p.refs.push({ name: branchRef(action.name), before: old, after: to });
        return p;
      }
      case "rewrite": {
        const old = c.branchesMap.get(action.branch) ?? null;
        if (!old) return { ...p, blocked: block("not-found", `There is no branch "${action.branch}".`) };
        const to = resolve(action.tip);
        if (!to) return { ...p, blocked: block("not-found", `The new tip "${action.tip}" does not exist.`) };
        if (to === old) return { ...p, undoable: false, blocked: block("nothing-to-do", `${action.branch} would not change.`) };
        const keep = c.ancestors(to);
        const leaving = [...c.ancestors(old)].filter((o) => !keep.has(o));
        const coming = [...keep].filter((o) => !c.ancestors(old).has(o));
        const onServer = new Set<string>();
        for (const t of c.fetched.values()) for (const a of c.ancestors(t)) onServer.add(a);
        const published = leaving.filter((o) => onServer.has(o)).length;
        if (published > 0) {
          stop(block("published", `${plural(published, "commit")} you would replace ${published === 1 ? "is" : "are"} on the server already. Rewriting history others may have is not done from here.`));
        }
        const summary = action.summary.trim().replace(/\.$/, "");
        if (summary) p.steps.push(`${summary}.`);
        p.steps.push(`Replaces ${plural(leaving.length, "commit")} on ${action.branch} with ${coming.length}; ${action.branch} then points at ${short(to)} ("${subjectOf(c.store.get(to)!)}").`);
        p.steps.push("The backup keeps the old commits for Undo.");
        p.refs.push({ name: branchRef(action.branch), before: old, after: to });
        p.backup = true;
        if (cur === action.branch) {
          p.files = diffPaths(c.store.get(old)!.tree, c.store.get(to)!.tree);
          stop(await unsaved());
          stop(inProgress());
          stop(localChanges("rewrite this branch"));
        }
        return p;
      }
      case "create-tag": {
        const problem = refNameProblem(action.name);
        const target = resolve(action.target);
        stop(problem ? block("invalid-name", problem) : null);
        stop(c.tagsMap.has(action.name) ? block("exists", `A tag named ${action.name} exists already.`) : null);
        stop(target ? null : block("not-found", `There is no commit "${action.target}".`));
        p.steps.push(`Names ${target ? short(target) : action.target} "${action.name}"${action.message?.trim() ? " with a message" : ""}. A tag never moves.`);
        p.refs.push({ name: `refs/tags/${action.name}`, before: null, after: target });
        return p;
      }
      case "delete-tag": {
        const t = c.tagsMap.get(action.name);
        if (!t) return { ...p, blocked: block("not-found", `There is no tag ${action.name}.`) };
        p.steps.push(`Deletes the tag ${action.name} on this computer.`);
        if (c.remoteTags.has(action.name)) p.warnings = ["The server keeps its copy of the tag."];
        p.refs.push({ name: `refs/tags/${action.name}`, before: t.oid, after: null });
        p.backup = true;
        return p;
      }
      case "push-tag": {
        p.network = true;
        p.undoable = false;
        stop(c.tagsMap.has(action.name) ? null : block("not-found", `There is no tag ${action.name}.`));
        stop(c.remoteUrls.has(action.remote) ? null : block("not-found", `There is no remote ${action.remote}.`));
        stop(c.remoteTags.has(action.name) ? block("nothing-to-do", `${action.remote} has the tag already.`) : null);
        p.steps.push(`Sends the tag ${action.name} to ${action.remote}.`);
        return p;
      }
      default:
        return { ...p, undoable: false, blocked: block("unsupported", "This Git action is not supported here.") };
    }
  }

  // ---------------------------------------------------------------- apply

  function backupRef(name: string, oid: string | null, t: number, out: string[]) {
    if (!oid) return;
    const ref = `refs/requesttailor/backup/${t}/${name}`;
    c.backups.set(ref, oid);
    out.push(ref);
  }

  /** Merges `theirsTree` into the working tree over `baseTree`; conflicts leave a merge in
   *  progress (the wave-12 conflict views take over). */
  function threeWay(
    baseTree: Tree,
    theirsTree: Tree,
    kind: "merge" | "cherry-pick" | "revert",
    theirs: string,
    baseOid: string | null,
    message: string,
  ): { outcome: "conflicts"; paths: string[] } | { merged: Tree } {
    const ours = clone(c.headTree());
    const files = mergeTrees(baseTree, ours, theirsTree);
    const merged = clone(ours);
    for (const f of files) {
      if (f.merged) merged.set(f.path, clone(f.merged));
      else merged.delete(f.path);
    }
    c.moveWork(ours, merged);
    const conflicted = files.filter((f) => f.conflicts.length > 0).map((f) => f.path);
    if (conflicted.length > 0) {
      c.mergeHead = theirs;
      c.mergeKind = kind;
      c.mergeMessage = message;
      c.mergeBaseOid = baseOid;
      for (const p of conflicted) c.conflicts.add(p);
      return { outcome: "conflicts" as const, paths: conflicted.sort() };
    }
    return { merged };
  }

  async function author(): Promise<string> {
    const who = await c.identity();
    if (!who) throw new BackendError("git", "Set your name and email for commits first.");
    return who.name;
  }

  async function run(action: GitAction, p: GitPlan, backups: string[], t: number): Promise<{ message: string; outcome?: GitPullResult; reload: boolean }> {
    const head = c.head();
    const cur = c.current;
    switch (action.action) {
      case "create-branch": {
        const start = c.resolveRev(action.start);
        c.branchesMap.set(action.name, start);
        if (action.switch) {
          backupRef("HEAD", head, t, backups);
          c.moveWork(clone(c.headTree()), c.store.get(start)!.tree);
          c.current = action.name;
          c.detachedAt = null;
          return { message: `Created ${action.name} and switched to it.`, reload: start !== head };
        }
        return { message: `Created ${action.name}.`, reload: false };
      }
      case "switch": {
        const t2 = action.target;
        const target = c.resolveRev(t2);
        backupRef("HEAD", head, t, backups);
        c.moveWork(clone(c.headTree()), c.store.get(target)!.tree);
        if (c.branchesMap.has(t2)) {
          c.current = t2;
          c.detachedAt = null;
        } else if (t2.startsWith("origin/") && c.fetched.has(t2.slice(7))) {
          const name = t2.slice(7);
          if (!c.branchesMap.has(name)) c.branchesMap.set(name, target);
          c.tracking.add(name);
          c.current = name;
          c.detachedAt = null;
          const tip = c.branchesMap.get(name);
          if (tip && tip !== target) c.moveWork(clone(c.store.get(target)!.tree), c.store.get(tip)!.tree);
        } else {
          c.current = null;
          c.detachedAt = target;
          return { message: `Showing ${t2}. You are not on a branch.`, reload: true };
        }
        return { message: `Switched to ${c.current}.`, reload: true };
      }
      case "rename-branch": {
        const at = c.branchesMap.get(action.from)!;
        c.branchesMap.delete(action.from);
        c.branchesMap.set(action.to, at);
        if (c.current === action.from) c.current = action.to;
        return { message: `Renamed ${action.from} to ${action.to}.`, reload: false };
      }
      case "delete-branch": {
        backupRef(action.name, c.branchesMap.get(action.name) ?? null, t, backups);
        c.branchesMap.delete(action.name);
        c.tracking.delete(action.name);
        return { message: `Deleted ${action.name}.`, reload: false };
      }
      case "merge": {
        const from = c.resolveRev(action.from);
        backupRef(cur!, head, t, backups);
        if (p.merge?.fast_forward) {
          c.moveWork(clone(c.headTree()), c.store.get(from)!.tree);
          c.branchesMap.set(cur!, from);
          return { message: `Moved ${cur} forward to ${action.from}.`, outcome: { outcome: "fast-forward", commits: p.merge.commits.length }, reload: true };
        }
        const who = await author();
        const message = `Merge ${action.from} into ${cur}`;
        const baseTree = c.store.get(c.baseOf(head!, from) ?? "")?.tree ?? new Map<string, RequestSpec>();
        const r = threeWay(baseTree, c.store.get(from)!.tree, "merge", from, null, message);
        if ("outcome" in r) return { message: `Merging ${action.from} needs your decision in ${plural(r.paths.length, "file")}.`, outcome: r, reload: true };
        const oid = c.newCommit([head!, from], r.merged, message, who);
        c.branchesMap.set(cur!, oid);
        return { message: `Merged ${action.from} into ${cur}.`, outcome: { outcome: "merged", oid }, reload: true };
      }
      case "pull": {
        backupRef(cur!, head, t, backups);
        const upstream = `origin/${cur}`;
        const r = await c.pull();
        const message =
          r.outcome === "up-to-date"
            ? "Already up to date."
            : r.outcome === "fast-forward"
              ? `Got ${plural(r.commits, "commit")} from ${upstream}.`
              : r.outcome === "merged"
                ? `Merged ${upstream}.`
                : `Conflicts in ${plural(r.paths.length, "file")}: choose a side for each, then commit.`;
        return { message, outcome: r, reload: r.outcome !== "up-to-date" };
      }
      case "push": {
        await c.push();
        return { message: `Sent ${cur} to origin/${cur}.`, reload: false };
      }
      case "publish": {
        if (action.remote !== "origin" || !c.remote) throw new BackendError("git", "This repository can only publish to origin here.");
        c.requireAuthFor(host("origin"));
        const at = c.branchesMap.get(action.branch)!;
        c.remote.set(action.branch, at);
        c.fetched.set(action.branch, at);
        c.tracking.add(action.branch);
        return { message: `Published ${action.branch} to ${action.remote}.`, reload: false };
      }
      case "fetch": {
        if (c.remote) {
          c.requireAuthFor(host(action.remote ?? "origin"));
          for (const [b, o] of c.remote) c.fetched.set(b, o);
        }
        return { message: "Checked the server for changes.", reload: false };
      }
      case "add-remote": {
        c.remoteUrls.set(action.name, action.url.trim());
        if (action.name === "origin" && !c.remote) c.remote = new Map();
        return { message: `Linked to ${action.url.trim()} as ${action.name}.`, reload: false };
      }
      case "remove-remote": {
        c.remoteUrls.delete(action.name);
        if (action.name === "origin") {
          c.remote = null;
          c.fetched.clear();
          c.tracking.clear();
        }
        return { message: `Removed the remote ${action.name}.`, reload: false };
      }
      case "rename-remote": {
        const url = c.remoteUrls.get(action.from)!;
        c.remoteUrls.delete(action.from);
        c.remoteUrls.set(action.to, url);
        return { message: `Renamed the remote ${action.from} to ${action.to}.`, reload: false };
      }
      case "set-remote-url": {
        c.remoteUrls.set(action.name, action.url.trim());
        return { message: `${action.name} now points to ${action.url.trim()}.`, reload: false };
      }
      case "stash": {
        const sel = selection(action.paths ?? []);
        const files = c.files().map((f) => f.path).filter(sel);
        const message = action.message?.trim() || `Parked on ${cur ?? "HEAD"}`;
        const whole = (action.paths ?? []).length === 0;
        const tree = whole ? clone(c.work) : clone(c.headTree());
        if (!whole) for (const f of files) (c.work.has(f) ? tree.set(f, clone(c.work.get(f)!)) : tree.delete(f));
        const oid = c.newCommit(head ? [head] : [], tree, message, "RequestTailor");
        c.stashList.unshift({ oid, message, time: t, branch: cur, files });
        if (whole) c.moveWork(clone(c.work), c.headTree());
        else for (const f of files) c.writeWork(f, c.headTree().get(f));
        return { message: `Parked ${plural(files.length, "change")}.`, reload: true };
      }
      case "unstash": {
        const s = c.stashList[action.index];
        const tree = c.store.get(s.oid)!.tree;
        backupRef("worktree", c.newCommit(head ? [head] : [], clone(c.work), "Backup of the working files", "RequestTailor"), t, backups);
        if (!action.keep) backupRef("stash", s.oid, t, backups);
        for (const f of s.files) c.writeWork(f, tree.get(f));
        if (!action.keep) c.stashList.splice(action.index, 1);
        return { message: `Brought back "${s.message}".`, reload: true };
      }
      case "drop-stash": {
        const [s] = c.stashList.splice(action.index, 1);
        backupRef("stash", s.oid, t, backups);
        return { message: `Deleted the parked changes "${s.message}".`, reload: false };
      }
      case "revert":
      case "cherry-pick": {
        const oid = c.resolveRev(action.commit);
        const commit = c.store.get(oid)!;
        const parent = commit.parents[0] ? c.store.get(commit.parents[0])!.tree : new Map<string, RequestSpec>();
        const pick = action.action === "cherry-pick";
        const subject = subjectOf(commit);
        const message = pick ? commit.message : `Revert "${subject}"`;
        const who = pick ? commit.author : await author();
        backupRef(cur!, head, t, backups);
        const r = pick
          ? threeWay(parent, commit.tree, "cherry-pick", oid, commit.parents[0] ?? null, message)
          : threeWay(commit.tree, parent, "revert", commit.parents[0], oid, message);
        if ("outcome" in r) {
          return { message: `${pick ? "Copying" : "Undoing"} "${subject}" needs your decision in ${plural(r.paths.length, "file")}.`, outcome: r, reload: true };
        }
        const created = c.newCommit([head!], r.merged, message, who);
        c.branchesMap.set(cur!, created);
        return {
          message: pick ? `Copied "${subject}" onto ${cur}.` : `Undid "${subject}" with a new commit.`,
          outcome: { outcome: "merged", oid: created },
          reload: true,
        };
      }
      case "reset": {
        const target = c.resolveRev(action.target);
        backupRef(cur!, head, t, backups);
        if (action.mode === "discard") {
          backupRef("worktree", c.newCommit(head ? [head] : [], clone(c.work), "Backup of the working files", "RequestTailor"), t, backups);
          c.moveWork(clone(c.work), c.store.get(target)!.tree);
        }
        c.branchesMap.set(cur!, target);
        return { message: `Moved ${cur} to ${short(target)}.`, reload: action.mode === "discard" };
      }
      case "discard": {
        backupRef("worktree", c.newCommit(head ? [head] : [], clone(c.work), "Backup of the working files", "RequestTailor"), t, backups);
        const headTree = c.headTree();
        for (const f of p.files ?? []) c.writeWork(unslash(f), headTree.get(unslash(f)));
        return { message: `Discarded the changes to ${plural((p.files ?? []).length, "file")}.`, reload: true };
      }
      case "amend": {
        const commit = c.store.get(head!)!;
        const sel = selection(action.paths ?? []);
        const tree = clone(commit.tree);
        if ((action.paths ?? []).length > 0) {
          for (const f of c.files()) if (f.state !== "conflicted" && sel(f.path)) (c.work.has(f.path) ? tree.set(f.path, clone(c.work.get(f.path)!)) : tree.delete(f.path));
        }
        const text = action.message?.trim() || commit.message;
        backupRef(cur!, head, t, backups);
        c.branchesMap.set(cur!, c.newCommit(commit.parents, tree, text, commit.author));
        return { message: `Replaced the last commit on ${cur} with "${text.split("\n")[0]}".`, reload: false };
      }
      case "abort": {
        backupRef("worktree", c.newCommit(head ? [head] : [], clone(c.work), "Backup of the working files", "RequestTailor"), t, backups);
        c.moveWork(clone(c.work), c.headTree());
        c.mergeHead = null;
        c.mergeKind = null;
        c.mergeMessage = null;
        c.mergeBaseOid = null;
        c.conflicts.clear();
        return { message: "Stopped it and went back to where it started.", reload: true };
      }
      case "restore-files": {
        const at = c.resolveRev(action.rev);
        const want = c.store.get(at)!.tree;
        backupRef("worktree", c.newCommit(head ? [head] : [], clone(c.work), "Backup of the working files", "RequestTailor"), t, backups);
        for (const f of p.files ?? []) c.writeWork(unslash(f), want.get(unslash(f)));
        return { message: `Restored ${plural((p.files ?? []).length, "file")} from ${short(at)}.`, reload: true };
      }
      case "restore-branch": {
        const to = c.resolveRev(action.target);
        backupRef(action.name, c.branchesMap.get(action.name) ?? null, t, backups);
        c.branchesMap.set(action.name, to);
        return { message: `${action.name} points at ${short(to)} again.`, reload: false };
      }
      case "rewrite": {
        const old = c.branchesMap.get(action.branch)!;
        const to = c.resolveRev(action.tip);
        backupRef(action.branch, old, t, backups);
        if (cur === action.branch) c.moveWork(clone(c.store.get(old)!.tree), c.store.get(to)!.tree);
        c.branchesMap.set(action.branch, to);
        const summary = action.summary.trim().replace(/\.$/, "");
        return { message: summary ? `${summary}.` : `Rewrote ${action.branch}.`, reload: cur === action.branch && (p.files ?? []).length > 0 };
      }
      case "create-tag": {
        const target = c.resolveRev(action.target);
        c.tagsMap.set(action.name, { oid: target, message: action.message?.trim() || null });
        return { message: `Tagged ${short(target)} as ${action.name}.`, reload: false };
      }
      case "delete-tag": {
        backupRef(`tags/${action.name}`, c.tagsMap.get(action.name)!.oid, t, backups);
        c.tagsMap.delete(action.name);
        return { message: `Deleted the tag ${action.name}.`, reload: false };
      }
      case "push-tag": {
        c.requireAuthFor(host(action.remote));
        c.remoteTags.add(action.name);
        return { message: `Sent the tag ${action.name} to ${action.remote}.`, reload: false };
      }
      default:
        throw new BackendError("git", "This Git action is not supported here.");
    }
  }

  /** Refs to record: what the plan names, `HEAD`, the current branch and the stash. */
  function refNames(p: GitPlan): string[] {
    const names = new Set(p.refs.map((r) => r.name).filter((n) => !n.startsWith("refs/remotes/")));
    names.add("HEAD");
    if (c.current) names.add(branchRef(c.current));
    names.add("refs/stash");
    return [...names];
  }

  function staleness(e: MockJournalEntry): string | undefined {
    const moved = e.entry.refs.filter((r) => refValue(r.name) !== r.after);
    if (moved.length === 0) return undefined;
    const names = moved.map((r) => (r.name === "HEAD" ? "the current commit" : r.name.replace(/^refs\/(heads|tags)\//, "")));
    return `${[...new Set(names)].join(", ")} changed since, so undoing would lose newer work.`;
  }

  return {
    async remotes(): Promise<GitRemote[]> {
      c.calls.push("remotes");
      c.start();
      return [...c.remoteUrls].map(([name, url]) => ({ name, url }));
    },
    async stashes(): Promise<GitStash[]> {
      c.calls.push("stashes");
      c.start();
      return c.stashList.map((s, index) => ({ index, oid: s.oid, message: s.message, time: s.time, branch: s.branch ?? undefined, files: s.files.map(slash) }));
    },
    async plan(action: GitAction): Promise<GitPlan> {
      c.calls.push(`plan ${action.action}`);
      return plan(action);
    },
    async apply(action: GitAction): Promise<GitActionResult> {
      c.calls.push(`apply ${action.action}`);
      const p = await plan(action);
      if (p.blocked) throw new BackendError("git", p.blocked.message);
      const names = refNames(p);
      const before = new Map(names.map((n) => [n, refValue(n)]));
      const snap = snapshot();
      const t = now();
      const backups: string[] = [];
      const done = await run(action, p, backups, t);
      if (!p.undoable) return { message: done.message, outcome: done.outcome, reload: done.reload };
      const all = [...new Set([...names, ...(c.current ? [branchRef(c.current)] : [])])];
      const refs: GitRefMove[] = all.map((name) => ({ name, before: before.get(name) ?? null, after: refValue(name) })).filter((r) => r.before !== r.after);
      const entry: GitUndoEntry = { id: `${t}-${++seq}`, action: action.action, summary: done.message, time: t, refs };
      if (backups.length) entry.backups = backups;
      c.journal.unshift({ entry, before: snap });
      c.journal.splice(20);
      return { message: done.message, undo: entry, outcome: done.outcome, reload: done.reload };
    },
    async lastAction(): Promise<GitUndoEntry | null> {
      c.start();
      const e = c.journal[0];
      if (!e) return null;
      const stale = staleness(e);
      return stale ? { ...e.entry, stale } : e.entry;
    },
    async undo(id: string): Promise<GitActionResult> {
      c.calls.push(`undo ${id}`);
      const at = c.journal.findIndex((e) => e.entry.id === id);
      if (at < 0) throw new BackendError("git", "This action can no longer be undone.");
      const e = c.journal[at];
      const stale = staleness(e);
      if (stale) throw new BackendError("git", `Cannot undo: ${stale}`);
      const blocked = await unsaved();
      if (blocked) throw new BackendError("git", blocked.message);
      const b = e.before;
      const headBefore = c.head();
      c.branchesMap.clear();
      for (const [k, v] of b.branches) c.branchesMap.set(k, v);
      c.current = b.current;
      c.detachedAt = b.detachedAt;
      c.tagsMap.clear();
      for (const [k, v] of b.tags) c.tagsMap.set(k, v);
      c.tracking.clear();
      for (const x of b.tracking) c.tracking.add(x);
      c.remoteUrls.clear();
      for (const [k, v] of b.remoteUrls) c.remoteUrls.set(k, v);
      if (!b.hadRemote) c.remote = null;
      else if (!c.remote) c.remote = new Map();
      c.stashList.splice(0, c.stashList.length, ...b.stashes);
      c.mergeHead = b.mergeHead;
      if (b.mergeHead === null) c.mergeKind = null;
      c.conflicts.clear();
      for (const x of b.conflicts) c.conflicts.add(x);
      let workChanged = false;
      for (const x of new Set([...c.work.keys(), ...b.work.keys()])) {
        if (!same(c.work.get(x), b.work.get(x))) {
          c.writeWork(x, b.work.get(x));
          workChanged = true;
        }
      }
      c.journal.splice(at, 1);
      return { message: `Undone: ${e.entry.summary}`, reload: workChanged || headBefore !== c.head() };
    },
    backupRefs: () => new Map(c.backups),
  };
}

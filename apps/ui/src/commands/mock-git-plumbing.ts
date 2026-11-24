// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand D: the commit building blocks on the mock's Git (`commitInfo`, `pick`,
// `writeTree`, `writeCommit`, `resolveText`). The mock keeps request files, not bytes, so
// trees here are request trees: `writeTree` takes `from` and `delete` (text and base64 need
// real files), and `resolveText` accepts a text only when it equals one of the sides.

import { BackendError } from "./backend";
import { mergeTrees, type Core } from "./mock-git-actions";
import { renderSpec, type MockGit } from "./mock-git";
import type { GitCommitDetails, GitConflict, GitPickResult, GitSignature, RequestSpec } from "./types";
import type { GitPlumbingMethods } from "./git-methods";

type Tree = Map<string, RequestSpec>;

const err = (message: string) => new BackendError("git", message);
const clone = <T>(v: T): T => structuredClone(v);
const unslash = (p: string) => p.replace(/^\/+/, "");

export function mockGitPlumbing(git: MockGit): GitPlumbingMethods {
  const c = git as unknown as Core;
  const trees = new Map<string, Tree>();
  let seq = 0;

  const rev = (r: string): string => {
    c.start();
    try {
      return c.resolveRev(r);
    } catch {
      throw err(`Unknown revision "${r}".`);
    }
  };
  const sig = (name: string, time: number): GitSignature => ({ name, email: `${name.toLowerCase().replace(/\W+/g, ".")}@example.com`, time, offset: 0 });
  const treeId = (oid: string) => `tree:${oid}`;
  /** A tree id, or any commit rev. */
  const treeOf = (r: string): Tree => {
    const known = trees.get(r);
    if (known) return known;
    if (r.startsWith("tree:")) {
      const commit = c.store.get(r.slice(5));
      if (commit) return commit.tree;
    }
    return c.store.get(rev(r))!.tree;
  };
  const keep = (tree: Tree): string => {
    const id = `wt${++seq}`;
    trees.set(id, tree);
    return id;
  };
  const text = (spec: RequestSpec | undefined) => (spec ? renderSpec(spec) : null);

  return {
    async commitInfo(r): Promise<GitCommitDetails> {
      c.calls.push(`commitInfo ${r}`);
      const oid = rev(r);
      const commit = c.store.get(oid)!;
      const who = sig(commit.author, commit.time);
      return { oid, tree: treeId(oid), parents: [...commit.parents], message: commit.message, author: who, committer: who, signed: false };
    },

    async pick(request): Promise<GitPickResult> {
      c.calls.push(`pick ${request.commit}`);
      const onto = c.store.get(rev(request.onto))!;
      const oid = rev(request.commit);
      const commit = c.store.get(oid)!;
      const parentOid = commit.parents[request.mainline ? request.mainline - 1 : 0] ?? null;
      if (commit.parents.length > 1 && !request.mainline) throw err("This is a merge commit: say which parent counts as the mainline.");
      if (request.revert && !parentOid) throw err("The first commit cannot be reverted.");
      const parent = parentOid ? c.store.get(parentOid)!.tree : new Map<string, RequestSpec>();
      const files = request.revert ? mergeTrees(commit.tree, onto.tree, parent) : mergeTrees(parent, onto.tree, commit.tree);
      const merged: Tree = clone(onto.tree);
      const conflicts: GitConflict[] = [];
      for (const f of files) {
        if (f.conflicts.length > 0) {
          const [base, theirs] = request.revert ? [commit.tree, parent] : [parent, commit.tree];
          conflicts.push({ path: `/${f.path}`, base: text(base.get(f.path)), ours: text(onto.tree.get(f.path)), theirs: text(theirs.get(f.path)) });
        } else if (f.merged) merged.set(f.path, clone(f.merged));
        else merged.delete(f.path);
      }
      return { tree: keep(merged), clean: conflicts.length === 0, ...(conflicts.length ? { conflicts } : {}) };
    },

    async writeTree(write): Promise<string> {
      c.calls.push("writeTree");
      const tree = clone(treeOf(write.base));
      for (const file of write.files) {
        const path = unslash(file.path);
        if (!path || path.split("/").some((x) => x === "" || x === "." || x === "..")) throw err(`"${file.path}" is not a path in the repository.`);
        if (file.kind === "delete") tree.delete(path);
        else if (file.kind === "from") {
          const spec = treeOf(file.rev).get(path);
          if (spec) tree.set(path, clone(spec));
          else tree.delete(path);
        } else throw err("The demo keeps request files only: use a revision or delete a file.");
      }
      return keep(tree);
    },

    async writeCommit(write): Promise<string> {
      c.calls.push("writeCommit");
      const tree = treeOf(write.tree);
      const who = (await c.identity())?.name;
      const name = write.author?.name ?? who;
      if (!name) throw err("Set your name and email for commits first.");
      return c.newCommit(write.parents.map(rev), clone(tree), write.message, name);
    },

    async resolveText(path, value): Promise<void> {
      c.calls.push(`resolveText ${path}`);
      c.start();
      const file = unslash(path);
      if (!c.mergeHead || !c.conflicts.has(file)) throw err(`${path} has no conflict.`);
      if ((await c.dirtyFiles()).length > 0) throw err("Save or discard your changes before you resolve a conflict.");
      const sides = [c.head()!, c.mergeHead].map((oid) => c.store.get(oid)!.tree.get(file));
      const hit = sides.find((s) => text(s) === value);
      if (hit === undefined && !sides.some((s) => s === undefined && value === "")) throw err("The demo can only take one of the two sides as the result.");
      c.writeWork(file, hit ? clone(hit) : undefined);
      c.conflicts.delete(file);
    },
  };
}

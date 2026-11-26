// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// `GitRepositoryApi` of the mock: the repositories of the set. The demo repository (the one
// `MockGit` stands for, `MockFixtures.repository`) is there from the start; `init` adds an empty
// one with the name `nextInit`, `clone` shows the demo repository again (the mock clone opens it,
// with the credential rules `MockGit.clone` has). A product with its own repository set
// (RequestTailor: derived from its collections) brings its own.

import type { GitRepositoryApi } from "../api";
import { BackendError } from "../api";
import type { GitRepoSummary, RepositoryEntry } from "../types";
import type { MockGit } from "./git";

export interface MockRepositories extends GitRepositoryApi {
  /** The repositories of the set, in order. */
  list(): RepositoryEntry[];
  /** The active one (`null` when the set is empty). */
  active(): string | null;
  /** The folder `init` picks next (`null` cancels). */
  nextInit: string | null;
}

export function mockRepositories<S>(git: () => MockGit<S>): MockRepositories {
  const entries: RepositoryEntry[] = [];
  let active: string | null = null;
  const entry = (id: string, name: string): RepositoryEntry => ({ id, name, location: id, root: id, linked: false });
  const ensureDemo = (): void => {
    const id = git().fixtures.repository;
    if (!entries.some((e) => e.id === id)) entries.unshift(entry(id, id.replace(/^.*[/:]/, "")));
    active ??= id;
  };
  const find = (id: string): RepositoryEntry => {
    ensureDemo();
    const found = entries.find((e) => e.id === id);
    if (!found) throw new BackendError("not-found", `no repository ${id} in the set`);
    return found;
  };
  const added = new Map<string, GitRepoSummary>();
  const api: MockRepositories = {
    nextInit: "new-repo",
    list: () => {
      ensureDemo();
      return structuredClone(entries);
    },
    active: () => {
      ensureDemo();
      return active;
    },
    async activate(id) {
      const found = find(id);
      active = found.id;
      return structuredClone(found);
    },
    async summary(id): Promise<GitRepoSummary> {
      const found = find(id);
      const own = added.get(found.id);
      if (own) return { ...own };
      const s = await git().status();
      return {
        id: found.id,
        branch: s.branch,
        upstream: s.upstream,
        ahead: s.ahead,
        behind: s.behind,
        changes: s.files.length,
        ...(s.merging ? { operation: "merge" as const } : {}),
      };
    },
    async clone(request) {
      await git().clone(request.url);
      const found = find(git().fixtures.repository);
      active = found.id;
      return structuredClone(found);
    },
    async init() {
      ensureDemo();
      const name = api.nextInit;
      if (!name) return null;
      const id = `mock://${name}`;
      if (!entries.some((e) => e.id === id)) {
        entries.push(entry(id, name));
        added.set(id, { id, branch: null, upstream: null, ahead: 0, behind: 0, changes: 0 });
      }
      active = id;
      return structuredClone(find(id));
    },
  };
  return api;
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Pure helpers of "Create pull request" (wave 15, strand E): which branch to offer, whether it is
// on the server, a title from the branch name, what the form still lacks.

import type { GitRef, NewPullRequest, ReviewRepo } from "../commands/types";

/** How a local branch relates to the server: `unpublished` (no branch there), `ahead` (the
 *  server has it, but fewer commits), `published` (a pull request can be opened now). */
export type PublishState = "unpublished" | "ahead" | "published";

/** Local branch names, current one first. */
export function localBranches(refs: readonly GitRef[]): GitRef[] {
  return refs.filter((r) => r.kind === "local").sort((a, b) => Number(b.current) - Number(a.current));
}

/** The name of `ref` on the remote (`origin/main` → `main`). */
function remoteName(ref: GitRef): string {
  return ref.name.slice(ref.name.indexOf("/") + 1);
}

/** Branch names on the server, without `HEAD`, each once. */
export function remoteBranches(refs: readonly GitRef[]): string[] {
  const names = refs.filter((r) => r.kind === "remote" && !r.name.endsWith("/HEAD")).map(remoteName);
  return [...new Set(names)];
}

/** Whether `head` is on the server and has nothing the server lacks. */
export function publishState(refs: readonly GitRef[], head: string): PublishState {
  const onServer = remoteBranches(refs).includes(head);
  if (!onServer) return "unpublished";
  const local = refs.find((r) => r.kind === "local" && r.name === head);
  return local && local.ahead > 0 ? "ahead" : "published";
}

/** The branch a pull request most likely goes into: `main`, `master`, `develop`, else the
 *  first branch on the server that is not `head`. */
export function defaultBase(remote: readonly string[], local: readonly string[], head: string): string {
  const pool = remote.length > 0 ? remote : local;
  const others = pool.filter((b) => b !== head);
  for (const preferred of ["main", "master", "develop"]) {
    if (others.includes(preferred)) return preferred;
  }
  return others[0] ?? "";
}

/** A title from a branch name: `feature/add-currency` becomes "Add currency". */
export function titleFromBranch(branch: string): string {
  const last = branch.split("/").filter(Boolean).pop() ?? branch;
  const words = last.replace(/[-_]+/g, " ").trim();
  return words ? words[0].toUpperCase() + words.slice(1) : "";
}

/** What the form still lacks, as one sentence for the user; `null`: ready. */
export function problem(pull: NewPullRequest, state: PublishState): string | null {
  if (!pull.head) return "Choose the branch with your changes.";
  if (!pull.base) return "Choose the branch to merge into.";
  if (pull.head === pull.base) return "Choose two different branches.";
  if (state === "unpublished") return `${pull.head} is not on the server yet.`;
  if (pull.title.trim() === "") return "Give it a title.";
  return null;
}

/** "pull request" (GitHub) or "merge request" (GitLab). */
export function pullWord(repo: ReviewRepo | null): string {
  return repo?.kind === "gitlab" ? "merge request" : "pull request";
}

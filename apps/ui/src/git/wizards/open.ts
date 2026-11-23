// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14 interface: how any view opens a Git wizard (the graph's context menus, the Git panel,
// the palette, the "diverged" and "detached HEAD" banners). A wizard is a dialog of the shell's
// dialog slot (`AppState.showDialog`); it plans first (`GitApi.plan`), shows the plan in plain
// words, applies on confirmation and offers "Undo" afterwards. Strand G owns the wizards
// (`GitWizard.svelte` and what it shows); the request shape is the interface.

import type { AppState } from "../../state/app.svelte";
import type { GitAction, GitActionResult } from "../../commands/types";
import GitWizard from "./GitWizard.svelte";

/** The wizards, by the action they end in. Some wizards choose between actions (`diverged`:
 *  merge or take theirs; `detached`: new branch here or back to a branch; `clone`). */
export type GitWizardKind = GitAction["action"] | "diverged" | "detached" | "clone" | "remotes" | "undo";

export interface GitWizardRequest {
  kind: GitWizardKind;
  /** Fields known already (stable; the graph and the panel rely on them):
   *  create-branch `{ start, name?, switch? }`, switch `{ target }`, merge `{ from }`,
   *  rename-branch `{ from }`, delete-branch `{ name | branch }`, publish `{ name | branch, remote? }`,
   *  cherry-pick and revert `{ commit }`, reset `{ target, mode?: "keep-changes" | "discard" }`,
   *  create-tag `{ target, name?, message? }`, delete-tag and push-tag `{ name }`,
   *  discard `{ paths }` (relative to the collection, or to the repository with a leading `/`), unstash and drop-stash `{ index }`,
   *  diverged `{ choice: "merge" | "theirs" }`, detached `{ choice: "branch" | "back" }`. */
  preset?: Record<string, unknown>;
}

const TITLES: Record<GitWizardKind, string> = {
  "create-branch": "New branch",
  switch: "Switch branch",
  "rename-branch": "Rename branch",
  "delete-branch": "Delete branch",
  merge: "Merge",
  pull: "Get changes",
  push: "Send changes",
  publish: "Publish branch",
  fetch: "Check for changes",
  "add-remote": "Add remote",
  "remove-remote": "Remove remote",
  "rename-remote": "Rename remote",
  "set-remote-url": "Change remote address",
  stash: "Park changes",
  unstash: "Bring back parked changes",
  "drop-stash": "Delete parked changes",
  revert: "Undo a commit",
  "cherry-pick": "Copy a commit here",
  reset: "Move branch back",
  discard: "Discard changes",
  "create-tag": "New tag",
  "delete-tag": "Delete tag",
  "push-tag": "Publish tag",
  diverged: "Branches went apart",
  detached: "Not on a branch",
  clone: "Clone repository",
  remotes: "Remotes",
  undo: "Undo last Git action",
};

export function wizardTitle(kind: GitWizardKind): string {
  return TITLES[kind];
}

/** Opens the wizard; resolves with what it did, `undefined` when cancelled. */
export function openGitWizard(app: AppState, request: GitWizardRequest): Promise<GitActionResult | undefined> {
  return app.showDialog({
    title: wizardTitle(request.kind),
    component: GitWizard,
    props: { request },
    width: 560,
  }) as Promise<GitActionResult | undefined>;
}

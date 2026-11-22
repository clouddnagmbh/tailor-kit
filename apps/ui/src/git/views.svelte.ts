// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 12 interface: the Git-for-APIs views in the main area. One view at a time; each strand
// opens its own kind (from its palette entries, `<strand>/commands.ts`, or from another view)
// and `GitViewHost.svelte` shows the strand's component for it. Adding a kind is an interface
// change; what a view shows is the strand's.

import { getContext, setContext } from "svelte";

import type { DiffScope } from "../commands/types";

export type GitView =
  /** Strand A: `diff/SemanticDiffView.svelte`. */
  | { kind: "diff"; scope: DiffScope }
  /** Strand B: `merge/MergeView.svelte`. */
  | { kind: "merge" }
  /** Strand C: `compare/ComparePanel.svelte`. */
  | { kind: "compare"; from?: string; to?: string }
  /** Strand C: `compare/BisectPanel.svelte`. */
  | { kind: "bisect"; good?: string; bad?: string }
  /** Strand D: `timeline/RequestTimeline.svelte`. */
  | { kind: "timeline"; path: string }
  /** Strand E: `review/ReviewPanel.svelte` (`number`: open this PR). */
  | { kind: "review"; number?: number }
  /** Strand F: `contract/ContractPanel.svelte`. */
  | { kind: "contract" }
  /** Strand G: `rescue/RescueWizard.svelte`. */
  | { kind: "rescue" };

export class GitViews {
  current = $state<GitView | null>(null);

  open(view: GitView): void {
    this.current = view;
  }

  close(): void {
    this.current = null;
  }
}

const KEY = Symbol("rt-git-views");

export function provideGitViews(views: GitViews): GitViews {
  return setContext(KEY, views);
}

/** The shell's views; components below `AppShell` only. */
export function getGitViews(): GitViews {
  const views = getContext<GitViews | undefined>(KEY);
  if (!views) throw new Error("getGitViews() outside AppShell");
  return views;
}

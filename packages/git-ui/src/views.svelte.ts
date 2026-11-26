// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 12 interface: the Git-for-APIs views in the main area. One view at a time; each strand
// opens its own kind (from its palette entries, `<strand>/commands.ts`, or from another view)
// and `GitViewHost.svelte` shows the strand's component for it. Adding a kind is an interface
// change; what a view shows is the strand's.

import { getContext, setContext } from "svelte";

import type { DiffScope, GitHistoryQuery } from "./types";

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
  | { kind: "rescue" }
  /** Wave 14, strand F: `graph/GraphView.svelte`, the commit graph of the whole repository
   *  (`select`: a commit id or full ref name to select and scroll to). */
  | { kind: "graph"; select?: string }
  // Wave 15 (interface): the Git client for any repository; paths repository-relative with a
  // leading `/`.
  /** Strand B: `changes/ChangesView.svelte`, the working copy of the active repository. */
  | { kind: "changes" }
  /** Strand B: `files/FileView.svelte`, a file at `rev` (absent: the working tree), images and
   *  binary files included. */
  | { kind: "file"; path: string; rev?: string }
  /** Strand C: `history/FileHistoryView.svelte`, every commit that changed `path`. */
  | { kind: "file-history"; path: string }
  /** Strand C: `history/BlameView.svelte`. */
  | { kind: "blame"; path: string; rev?: string }
  /** Strand C: `history/SearchView.svelte`, searching messages, authors, paths and content. */
  | { kind: "search"; query?: Partial<GitHistoryQuery> }
  /** Strand C: `history/ReflogView.svelte` (`name` absent: `HEAD`), bringing back lost work. */
  | { kind: "reflog"; name?: string }
  /** Strand D: `conflict/TextMergeView.svelte`, the 3-way view of a conflicted file that is not
   *  a collection file. */
  | { kind: "text-merge"; path: string }
  /** Strand E: `extension/ExtensionGitView.svelte`, an extension's view
   *  (`AppExtension.gitViews`, `id` `<extension>/<view>`). */
  | { kind: "extension"; id: string; props?: Record<string, unknown> };

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

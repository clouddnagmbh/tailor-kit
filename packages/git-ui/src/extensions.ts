// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// What a product's extensions contribute to the Git views: actions, views, a staging view, diff and
// merge editors, commit form actions, hints (provenance: RequestTailor `apps/ui/src/extensions.ts`,
// moved here in step K3). The product keeps its own extension system; the views see only the Git
// part through `GitExtension`, which the product's extension type satisfies structurally, and
// `GitHost.extensions`. Generic Git: nothing here assumes a collection. Paths are
// repository-relative with a leading `/`.

import type { Component } from "svelte";

import type { GitConflict, GitWorkingCopy } from "./types";

/** The product's extension context, passed to its extensions untouched; the package never reads it. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type GitExtensionContext = any;

/** An action on a request, a response or a run (since wave 16). */
export interface ExtensionAction {
  /** Unique within the extension. */
  id: string;
  label: string;
  icon?: string;
  /** Shown as a button (at most two per place, the first ones); else in the "More" menu. */
  inline?: boolean;
  run: () => unknown;
}

/** Two revisions selected together (since wave 16): two commits or tags in the commit graph, or
 *  the sides of the comparison and of the diff of two revisions. */
export interface RevisionRangeTarget {
  /** The active owner (`GitOwner.id`), if the repository has one. A product that wants more
   *  fields on the target adds them with `ProvidedGitExtensions.rangeTarget`. */
  owner: string | null;
  /** Older side: a commit, branch or tag. */
  from: string;
  /** Newer side. */
  to: string;
  source: "graph" | "compare" | "diff";
}

/** The Git views a hint can sit in (since wave 15). */
export type GitHintView = "graph" | "changes" | "conflict" | "history";

/** At the top of a Git view of repository `repository` (`AppExtension.gitHints`, since wave
 *  15); same rules as `HintTarget`. */
export interface GitHintTarget {
  place: "git";
  repository: string;
  view: GitHintView;
  // What the user is doing in the view, for hints that only fit one situation. The view that
  // shows the hint fills what it can tell; absent: unknown. Dismissals do not depend on them.
  /** `graph`: how many commits are selected together (1 = just the one, 2 = a range). */
  selected?: number;
  /** `graph`: a commit row is being dragged right now. */
  dragging?: boolean;
  /** `changes`: the largest number of hunks (separate blocks of changed lines) in one changed
   *  text file among the files shown so far; a file counts once it has been opened. */
  hunks?: number;
}

/** A small, calm note the shell shows in a defined place (since wave 14): never a modal, never
 *  blocking, always dismissible. The shell shows at most one hint per place at a time (the
 *  first not dismissed, extensions in order) and remembers a dismissal per device. */
export interface ExtensionHint {
  /** Unique within the extension; dismissals are stored under `<extension id>/<hint id>`. */
  id: string;
  /** One or two short sentences, plain text. */
  text: string;
  /** Up to two buttons; running one does not dismiss the hint unless `dismiss` is set. */
  actions?: readonly { label: string; run: () => unknown; dismiss?: boolean }[];
  /** `device` (default): once dismissed, never again on this device; `target`: dismissed for
   *  this collection or request only. */
  scope?: "device" | "target";
}

/** What a Git action of an extension is offered on. `repository` is `RepositoryEntry.id`. */
export type GitActionTarget =
  /** The repository as a whole: its sidebar node's menu, the Git panel's "Tools" section. */
  | { kind: "repository"; repository: string }
  /** One commit in the graph or a history list. */
  | { kind: "commit"; repository: string; commit: string }
  /** Several commits selected in the graph, newest first. */
  | { kind: "commits"; repository: string; commits: string[] }
  /** A branch, tag or remote branch (`GitRef.full`). */
  | { kind: "ref"; repository: string; ref: string }
  /** A file in the changes view, the file tree or a history list; `rev` absent: working tree. */
  | { kind: "file"; repository: string; path: string; rev?: string }
  /** The changed files of the working copy as a whole. */
  | { kind: "changes"; repository: string; paths: string[] }
  /** A parked change (`GitStash.index`). */
  | { kind: "stash"; repository: string; index: number }
  /** A conflicted file during a merge, cherry-pick, revert or rebase. */
  | { kind: "conflict"; repository: string; path: string };

/** A view in the main area's Git host, opened with `AppState.show({ kind: "git", view: {
 *  kind: "extension", id: "<extension>/<view>", props } })` (since wave 15). */
export interface ExtensionGitView<Ctx = GitExtensionContext> {
  /** Unique within the extension. */
  id: string;
  /** Title bar of the view. */
  title: string;
  /** Semantic icon name (`ui/icons.ts`); default `graph`. */
  icon?: string;
  component: Component<ExtensionGitViewProps<Ctx>>;
}

export interface ExtensionGitViewProps<Ctx = GitExtensionContext> {
  ctx: Ctx;
  repository: string;
  /** The `props` the view was opened with. */
  props: Record<string, unknown>;
  /** Closes the view (back to the previous one). */
  close: () => void;
}

/** A staging view that replaces the file list of the app's commit form (since wave 15). With
 *  one, the form commits what the index holds (`GitApi.commitStaged`); without, the app keeps
 *  "commit without staging". The first extension's staging view wins. */
export interface ExtensionStaging<Ctx = GitExtensionContext> {
  /** Shown as the file list's heading. */
  title: string;
  component: Component<ExtensionStagingProps<Ctx>>;
  /** Whether it is in charge for this repository right now (e.g. allowed to); called reactively:
   *  cheap, synchronous, no `$state` writes. Absent: always. */
  active?(ctx: Ctx, repository: string): boolean;
}

export interface ExtensionStagingProps<Ctx = GitExtensionContext> {
  ctx: Ctx;
  repository: string;
  /** The working copy as last read; `refresh()` reads it again (after staging). */
  workingCopy: GitWorkingCopy;
  refresh: () => Promise<void>;
}

/** An editor for a file's diff or a conflicted file's merge, offered next to the app's own
 *  (since wave 15): the app's line and semantic views stay the default, the user can switch. */
export type ExtensionFileEditor<Ctx = GitExtensionContext> =
  | {
      id: string;
      title: string;
      kind: "diff";
      /** Which files it can show; absent: all text files. */
      accepts?(path: string): boolean;
      component: Component<ExtensionDiffEditorProps<Ctx>>;
    }
  | {
      id: string;
      title: string;
      kind: "merge";
      accepts?(path: string): boolean;
      component: Component<ExtensionMergeEditorProps<Ctx>>;
    };

/** What a diff editor gets: a file's two sides, read-only context in the app's own view. The
 *  host shows it in place of its own diff (the user switches back with the same control);
 *  `close()` returns to the host's own diff. */
export interface ExtensionDiffEditorProps<Ctx = GitExtensionContext> {
  ctx: Ctx;
  repository: string;
  /** Repository-relative, with a leading `/`. */
  path: string;
  /** Revs of both sides; `to` `null`: the working tree. */
  from: string;
  to: string | null;
  close: () => void;
}

/**
 * What a merge editor gets. The contract of `resolve` and `close`:
 *
 * - `resolve(text)` writes `text` as the file's content and marks it resolved
 *   (`GitApi.resolveText`, which refuses while there are unsaved edits and says why), reloads
 *   the app's collections, and then **closes the editor itself**: the host returns to where the
 *   user came from. It resolves when all of that is done and rejects with the reason when not;
 *   the editor stays open then, so the user can retry or save first. Show the rejection's
 *   `message`.
 * - `close()` leaves the editor without resolving anything. Calling it after a successful
 *   `resolve` is harmless (the host closes once), so an editor may also call `close()` itself
 *   after `resolve` and does not have to.
 * - The editor is shown only for the file it was opened for; `conflict` holds the three
 *   versions (`base` the common ancestor, `ours`, `theirs`; absent = deleted on that side).
 */
export interface ExtensionMergeEditorProps<Ctx = GitExtensionContext> {
  ctx: Ctx;
  repository: string;
  conflict: GitConflict;
  resolve: (text: string) => Promise<void>;
  close: () => void;
}

/** The commit form an action works on (since wave 15). */
export interface CommitFormTarget {
  repository: string;
  /** The message field. */
  message: { get(): string; set(text: string): void };
  /** Files that will be committed (repository-relative). */
  paths: readonly string[];
  /** The form commits the index (a staging view is in charge). */
  staged: boolean;
}

/** The Git part of an extension: what `AppExtension` of RequestTailor offers next to its own
 *  contributions. A product's extension type is assignable to it. */
export interface GitExtension<Ctx = GitExtensionContext> {
  /** Unique among the product's extensions. */
  id: string;
  /** Actions on a thing in the Git views. */
  gitActions?(ctx: Ctx, target: GitActionTarget): readonly ExtensionAction[];
  /** Views the product's Git host can show. */
  gitViews?(ctx: Ctx): readonly ExtensionGitView<Ctx>[];
  /** A staging view that replaces the file list of the commit form. */
  staging?: ExtensionStaging<Ctx>;
  /** Diff and merge editors, next to the views' own. */
  fileEditors?(ctx: Ctx): readonly ExtensionFileEditor<Ctx>[];
  /** Hints at the top of the Git views. */
  gitHints?(ctx: Ctx, target: GitHintTarget): readonly ExtensionHint[];
  /** Actions next to the commit message. */
  commitFormActions?(ctx: Ctx, target: CommitFormTarget): readonly ExtensionAction[];
  /** Actions on two revisions selected together (graph). */
  rangeActions?(ctx: Ctx, target: RevisionRangeTarget): readonly ExtensionAction[];
}

/** The product's extensions with the context they are called with (`GitHost.extensions`). */
export interface ProvidedGitExtensions<Ctx = GitExtensionContext> {
  readonly extensions: readonly GitExtension<Ctx>[];
  readonly ctx: Ctx;
  /** Adds the product's own fields to a range target before extensions see it (RequestTailor:
   *  `collection`); absent: the target as the views built it. */
  rangeTarget?(target: RevisionRangeTarget): RevisionRangeTarget;
}

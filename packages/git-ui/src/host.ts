// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Seam N6 (UI host) and N9 (brand strings): everything the Git views need from the product around
// them, and nothing else. A view gets it with `getGitHost()`; the product puts it there once with
// `setGitHost()` at its root. The views never import the product's state. The generic part (Git,
// repositories, pull requests, dialogs, notices) is required; `content` carries what only a product
// with files of its own (RequestTailor: collections) can offer. Without `content` the views fall
// back to plain Git: line diffs, no "open its collection" buttons, no field-by-field merge.
//
// Moved from RequestTailor's `apps/ui/src/git/host.ts` in step K3.

import type { Command, HintStore, ShellStorage } from "@tailor-kit/shell";
import { getContext, setContext, type Component } from "svelte";

import type { GitHostBackend, GitOwnerInfo } from "./api";
import type { ProvidedGitExtensions } from "./extensions";
import type { CollectionDiff, MergeState, RepositoryEntry, ReviewRepo, TreeNode } from "./types";
import type { GitView, GitViews } from "./views.svelte";

/** The parts of the product's name the views print or send. */
export interface GitBrand {
  /** "RequestTailor", "GitTailor": in warnings and wording that names the product. */
  productName: string;
  /** Drag and drop type of a commit dragged from the graph. */
  commitMime: string;
  /** `refs/<namespace>/backup/…`: where the product keeps its safety copies. */
  refNamespace: string;
}

/** What a dialog component gets from `GitHost.showDialog`. */
export interface GitDialogProps {
  /** Closes the dialog; `showDialog` resolves with `result`. */
  close: (result?: unknown) => void;
  /** The `props` passed to `showDialog`. */
  props: Record<string, unknown>;
}

/** A dialog the host shows: title bar, close button, Escape handling; one at a time (a second
 *  replaces the first, which resolves `undefined`). */
export interface GitDialog {
  title: string;
  component: Component<GitDialogProps>;
  props?: Record<string, unknown>;
  /** Width in CSS pixels. */
  width?: number;
}

/** Something that owns files of the repository (RequestTailor: a collection). */
export interface GitOwner {
  id: string;
  name: string;
  /** For display only. */
  location: string;
  /** The repository that holds it. */
  repo: string | null;
}

export interface GitMergeFieldsProps {
  merge: MergeState;
  /** Only this file. */
  only?: string;
  blocked: string | null;
  /** After a resolve, with the state that is left. */
  onresolved: (left: MergeState) => void;
}

export interface GitPullViewProps {
  number: number;
  repo: ReviewRepo;
  onback: () => void;
  /** The host rejected the token (or there is none): the list asks for one again. */
  onauth: (message: string | null) => void;
}

/** The field-level diffs a product with owned content can compute. */
export interface GitContentDiffs {
  /** The saved files against `HEAD`. */
  working(): Promise<CollectionDiff>;
  /** A commit against its first parent. */
  commit(rev: string): Promise<CollectionDiff>;
  /** `from` → `to` (branch compare, PR review: merge base → head). */
  between(from: string, to: string): Promise<CollectionDiff>;
}

/** The field-level merge state of a product with owned content. */
export interface GitContentMerges {
  /** The conflicts of the merge in progress (recomputed from Git's index). */
  conflicts(): Promise<MergeState>;
}

/** What only a product with owned content (RequestTailor collections) adds. All reads are
 *  reactive when the product's state is. */
export interface GitContent {
  /** The active owner (`GitOwner.id`) or `null`. */
  readonly activeOwner: string | null;
  /** All owners, in sidebar order. */
  readonly owners: readonly GitOwner[];
  /** Makes `owner` the active one (the field view works on the active owner). */
  activate(owner: string): Promise<void>;
  /** The active owner's file tree, to name changed files; `null`: none. */
  readonly tree: TreeNode | null;
  /** The file the user has open, for "blame this request" and "file history of this request". */
  readonly activeFile: string | null;
  /** Semantic diffs (`CollectionDiff`) of the working copy, a commit and two revisions. */
  readonly diffs: GitContentDiffs;
  /** The merge in progress, per field. */
  readonly merges: GitContentMerges;
  /** Writes the unsaved edits (the way out of a block caused by `GitHost.unsaved`). */
  save(): Promise<void>;
  /** Shows the owner a clone created (the old `git.clone`). */
  adopt(info: GitOwnerInfo): Promise<void>;
  readonly components: {
    /** A field-by-field diff of `CollectionDiff`. */
    SemanticDiff: Component<{ diff: CollectionDiff }>;
    /** The per-field decisions of a merge. */
    MergeFields: Component<GitMergeFieldsProps>;
    /** One pull request, opened from the review list. */
    PullView: Component<GitPullViewProps>;
  };
}

export interface GitHost {
  readonly backend: GitHostBackend;
  readonly brand: GitBrand;

  // ---- state the views read ----
  /** `RepositoryEntry.id` of the repository every Git view acts on. */
  readonly activeRepository: string | null;
  readonly repositories: readonly RepositoryEntry[];
  /** Counts reloads of what Git changed underneath; views that cache read again when it changes. */
  readonly reloadCount: number;
  /** A key for caches that must be dropped when the shown repository state may belong to
   *  something else (RequestTailor: the active collection). */
  readonly cacheScope: string | null;
  /** A commit message an extension asked the commit form to show. */
  readonly commitPrefill: { text: string; seq: number } | null;
  /** A host dialog is open. */
  readonly dialogOpen: boolean;
  /** Paths of edits not yet on disk. Non-empty blocks checkout, pull and resolving. */
  unsaved(): readonly string[];

  // ---- actions ----
  /** Re-reads what Git changed underneath (checkout, pull, conflict resolution). */
  reload(): Promise<void>;
  /** Shows a Git view in the main area. */
  show(view: GitView): void;
  showDialog(dialog: GitDialog): Promise<unknown>;
  /** The commit form took the prefill (or has no use for it). */
  takeCommitPrefill(): string | null;
  /** Shows a repository the backend has just added (clone, init). */
  adoptRepository(entry: RepositoryEntry): Promise<void>;
  /** Puts a message in the host's status area. */
  notifyError(message: string): void;
  /** The palette entries and "Tools" tiles for this host. */
  commands(views: GitViews): Command[];

  // ---- the product's extensions and per-device settings ----
  /** The Git part of the product's extensions with their context; `null`: none. */
  readonly extensions: ProvidedGitExtensions | null;
  /** Per-device settings (the remembered editor of an extension). */
  readonly storage: ShellStorage;
  /** Which hints the user dismissed. */
  readonly hints: HintStore;

  /** Only a product with owned content provides it; the views work without. */
  readonly content?: GitContent;
}

const KEY = Symbol("git-host");

export function setGitHost(host: GitHost): GitHost {
  return setContext(KEY, host);
}

export function getGitHost(): GitHost {
  const host = getContext<GitHost | undefined>(KEY);
  if (!host) throw new Error("getGitHost() called outside a Git host");
  return host;
}

/** Whether Git has a repository to act on: the active repository, or an active owner. */
export function hasRepository(host: GitHost): boolean {
  return host.activeRepository !== null || (host.content?.activeOwner ?? null) !== null;
}

/** How a file that says it belongs to `owner` is shown. */
export interface FileOwnership {
  /** It belongs to the active owner: the field view applies (needs `content`). */
  semantic: boolean;
  /** It belongs to another owner the user can activate (needs `content`). */
  activatable: string | null;
  /** The other owner, when it is a known one. */
  other: GitOwner | undefined;
}

export function fileOwnership(host: GitHost, owner: string | null | undefined): FileOwnership {
  const content = host.content;
  if (!content || !owner) return { semantic: false, activatable: null, other: undefined };
  if (owner === content.activeOwner) return { semantic: true, activatable: null, other: undefined };
  return { semantic: false, activatable: owner, other: content.owners.find((o) => o.id === owner) };
}

/** The Git part of the product's extensions with their context, or `null` (`GitHost.extensions`). */
export function getGitExtensions(): ProvidedGitExtensions | null {
  return getGitHost().extensions;
}

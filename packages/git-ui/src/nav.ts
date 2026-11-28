// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Wave 8 (interface): one stop of the "back and forward" history of a window. It names a view of
// the main area and what is selected in it; going back reopens it and never changes a repository.
// The history itself (a list with a position) is the product's.

import type { GitView } from "./views.svelte";

/** What is selected inside a view; every field is optional. */
export interface NavSelection {
  /** A commit id or a full ref name. */
  commit?: string;
  /** A repository-relative path with a leading `/`. */
  path?: string;
  /** A 1-based line in `path`. */
  line?: number;
}

export interface NavEntry {
  /** The repository tab (its id). */
  tab: string;
  /** The view of the main area; `null`: the default one (changes or history). */
  view: GitView | null;
  selection: NavSelection;
}

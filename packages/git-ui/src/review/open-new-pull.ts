// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// How any view opens "Create pull request" (wave 15, strand E): the graph's branch menu, the
// Pull requests view and the palette. A dialog of the shell's dialog slot.

import type { PullRequest } from "../types";
import type { GitHost } from "../host";
import NewPullDialog from "./NewPullDialog.svelte";

/** What "Create pull request" can be opened with. All fields are optional. */
export type NewPullPrefill = {
  /** The branch with the changes (default: the current one). Ignored when it is not a local branch. */
  head?: string;
  /** The branch to merge into (default: the repository's usual one). */
  base?: string;
  /** Title (default: made from the branch name). The user can still edit it. */
  title?: string;
  /** Description (default: empty). The user can still edit it. */
  body?: string;
};

/** Resolves with the new pull request, `undefined` when cancelled. `head`: the branch with the
 *  changes (default: the current one), `base`: the branch to merge into, `title` and `body`
 *  prefill the form (since tailor-kit 0.6.1). */
export function openNewPull(host: GitHost, options: NewPullPrefill = {}): Promise<PullRequest | undefined> {
  return host.showDialog({
    title: "Create pull request",
    component: NewPullDialog,
    props: options,
    width: 560,
  }) as Promise<PullRequest | undefined>;
}

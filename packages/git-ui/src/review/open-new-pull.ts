// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// How any view opens "Create pull request" (wave 15, strand E): the graph's branch menu, the
// Pull requests view and the palette. A dialog of the shell's dialog slot.

import type { PullRequest } from "../types";
import type { GitHost } from "../host";
import NewPullDialog from "./NewPullDialog.svelte";

/** Resolves with the new pull request, `undefined` when cancelled. `head`: the branch with the
 *  changes (default: the current one), `base`: the branch to merge into. */
export function openNewPull(host: GitHost, options: { head?: string; base?: string } = {}): Promise<PullRequest | undefined> {
  return host.showDialog({
    title: "Create pull request",
    component: NewPullDialog,
    props: options,
    width: 560,
  }) as Promise<PullRequest | undefined>;
}

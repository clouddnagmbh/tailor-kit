// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Palette entries of pull-request review (wave 12, strand E). They open the review view
// (`GitViews`); the pull request is chosen there.

import type { GitHost } from "../git/host";
import type { GitViews } from "../git/views.svelte";
import type { Command } from "../shell/commands";
import { openNewPull } from "./open-new-pull";

export function reviewCommands(host: GitHost, views: GitViews): Command[] {
  return [
    {
      id: "git:review",
      label: "Review: pull and merge requests…",
      run: () => views.open({ kind: "review" }),
    },
    {
      id: "git:review-create",
      label: "Create pull request…",
      run: () => void openNewPull(host),
    },
  ];
}

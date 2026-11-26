// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand C: palette entries of the history views. Blame and file history of the
// active request need an open request; "Lost work" and "Search history" do not.

import type { Command } from "@tailor-kit/shell";
import type { GitHost } from "../host";
import type { GitViews } from "../views.svelte";

export function historyCommands(host: GitHost, views: GitViews): Command[] {
  const file = (open: (path: string) => void) => () => {
    const active = host.content?.activeFile;
    if (active) open(active);
    else host.notifyError("Open a request first.");
  };
  return [
    { id: "git-history-search", label: "Git: Search history", run: () => views.open({ kind: "search" }) },
    { id: "git-history-lost", label: "Git: Lost work (reflog)", run: () => views.open({ kind: "reflog" }) },
    { id: "git-history-blame", label: "Git: Blame this request", run: file((path) => views.open({ kind: "blame", path })) },
    { id: "git-history-file", label: "Git: File history of this request", run: file((path) => views.open({ kind: "file-history", path })) },
  ];
}

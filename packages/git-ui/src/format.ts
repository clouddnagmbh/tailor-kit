// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Texts of the Git view, kept out of the components so they are unit-tested and the same in
// both shells.

import type { DiffHunk, GitBranch, GitFileState, GitPullResult, GitStatus } from "./types";

export const STATE_LETTER: Record<GitFileState, string> = {
  untracked: "U",
  modified: "M",
  deleted: "D",
  conflicted: "C",
};

export const STATE_LABEL: Record<GitFileState, string> = {
  untracked: "new, not tracked yet",
  modified: "modified",
  deleted: "deleted",
  conflicted: "conflict: choose a side",
};

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export function pullMessage(result: GitPullResult, upstream: string | null): string {
  switch (result.outcome) {
    case "up-to-date":
      return "Already up to date.";
    case "fast-forward":
      return `Pulled ${plural(result.commits, "commit")} (fast-forward).`;
    case "merged":
      return `Merged ${upstream ?? "the upstream"}.`;
    case "conflicts":
      return `Conflicts in ${plural(result.paths.length, "file")}: choose a side for each, then commit.`;
  }
}

export function pushMessage(status: GitStatus | null): string {
  const branch = status?.branch ?? "the branch";
  return `Pushed ${branch} to ${status?.upstream ?? `origin/${branch}`}.`;
}

/** e.g. "↑1 ↓2 origin/main", or a hint when there is no upstream. */
export function syncLabel(status: GitStatus): string {
  if (!status.upstream) return status.branch ? "not pushed yet" : "";
  const counts = [status.ahead > 0 ? `↑${status.ahead}` : "", status.behind > 0 ? `↓${status.behind}` : ""]
    .filter(Boolean)
    .join(" ");
  return counts ? `${counts} ${status.upstream}` : `in sync with ${status.upstream}`;
}

/** What the branch indicator shows. */
export function branchLabel(status: GitStatus, branches: GitBranch[]): string {
  if (status.branch) return status.branch;
  const current = branches.find((b) => b.current && !b.remote);
  return current ? `${current.name} (no commits yet)` : "detached HEAD";
}

export function hunkHeader(h: DiffHunk): string {
  return `@@ -${h.old_start},${h.old_lines} +${h.new_start},${h.new_lines} @@`;
}

/** Default message for the commit that finishes a merge. */
export function mergeMessage(status: GitStatus): string {
  return `Merge ${status.upstream ?? "upstream"} into ${status.branch ?? "HEAD"}`;
}

export function commitMessage(count: number): string {
  return `Committed ${plural(count, "file")}.`;
}

/** Relative time of a commit for the log, e.g. "3 min ago". */
export function ago(unixSeconds: number, nowMs = Date.now()): string {
  const s = Math.max(0, Math.round(nowMs / 1000 - unixSeconds));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(unixSeconds * 1000).toISOString().slice(0, 10);
}

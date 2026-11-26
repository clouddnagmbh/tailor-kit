// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Pure helpers of the pull-request review (wave 12, strand E): comment groups, labels. What reads a
// collection's field diff (`changedRequestPaths`, `fieldForLine`, `withFields`) stays in
// RequestTailor (`apps/ui/src/review/pull-model.ts`).

import type { CiState, PullRequest, ReviewComment, ReviewRepo } from "../types";

/** Whether the OAuth apps of the product are registered on github.com and gitlab.com. Until the
 *  project lead has registered them and put their client ids in `rt_app::review::device`
 *  (`GITHUB_CLIENT_ID`, `GITLAB_CLIENT_ID`), the app offers only the token: set this to `true`
 *  in the same change as those constants. */
export const DEVICE_LOGIN_REGISTERED = false;

/** The device-code login is offered for `github.com` and `gitlab.com` only. */
export function deviceLoginOffered(repo: ReviewRepo, registered: boolean = DEVICE_LOGIN_REGISTERED): boolean {
  return registered && (repo.host === "github.com" || repo.host === "gitlab.com");
}

export interface CommentGroup {
  /** `null`: general comments. */
  file: string | null;
  comments: ReviewComment[];
}

/** General comments first, then one group per file, comments oldest first. */
export function groupComments(comments: ReviewComment[]): CommentGroup[] {
  const sorted = [...comments].sort((a, b) => a.created - b.created);
  const groups: CommentGroup[] = [];
  for (const file of [null, ...new Set(sorted.map((c) => c.path).filter((p): p is string => p !== null))]) {
    const inGroup = sorted.filter((c) => c.path === file);
    if (inGroup.length > 0) groups.push({ file, comments: inGroup });
  }
  return groups;
}

export const CI_LABEL: Record<CiState, string> = {
  pending: "Checks running",
  success: "Checks passed",
  failure: "Checks failed",
  neutral: "Checks neutral",
  none: "No checks",
};

/** "3 minutes ago", "2 days ago"; `now` in Unix seconds. */
export function relativeTime(unix: number, now: number = Math.floor(Date.now() / 1000)): string {
  const s = Math.max(0, now - unix);
  const unit = (n: number, name: string) => `${n} ${name}${n === 1 ? "" : "s"} ago`;
  if (s < 60) return "just now";
  if (s < 3600) return unit(Math.floor(s / 60), "minute");
  if (s < 86_400) return unit(Math.floor(s / 3600), "hour");
  if (s < 30 * 86_400) return unit(Math.floor(s / 86_400), "day");
  return unit(Math.floor(s / (30 * 86_400)), "month");
}

export function stateLabel(p: PullRequest): string {
  if (p.state === "merged") return "Merged";
  if (p.state === "closed") return "Closed";
  return p.draft ? "Draft" : "Open";
}

/** The word for a host's pull requests. */
export function requestWord(repo: ReviewRepo | null): string {
  return repo?.kind === "gitlab" ? "merge request" : "pull request";
}

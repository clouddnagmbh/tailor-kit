// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand C: the plain logic of the history views (blame, search, lost work), apart
// from the components so it is tested without a DOM.

import type { GitBlame, GitBlameHunk, GitGraphCommit, GitHistoryField, GitHistoryQuery, GitReflogEntry } from "../types";

/** The lines of a file's text as blame counts them: the final line break ends a line, it does
 *  not start another. */
export function textLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

export interface BlameLine {
  /** 1-based line number. */
  n: number;
  text: string;
  hunk: GitBlameHunk;
  /** The first line of its hunk: the commit is shown here. */
  first: boolean;
  /** 0 or 1, alternating between neighbouring hunks, for the band behind the gutter. */
  band: 0 | 1;
}

/** Every line of the file with the hunk it belongs to; `lines` may be shorter than the blame
 *  (a file that changed meanwhile): missing lines are empty. */
export function blameLines(blame: GitBlame, lines: readonly string[]): BlameLine[] {
  const out: BlameLine[] = [];
  blame.hunks.forEach((hunk, h) => {
    for (let k = 0; k < hunk.lines; k++) {
      const n = hunk.start + k;
      out.push({ n, text: lines[n - 1] ?? "", hunk, first: k === 0, band: (h % 2) as 0 | 1 });
    }
  });
  return out;
}

/** How many different commits the blame names. */
export function commitCount(blame: GitBlame): number {
  return new Set(blame.hunks.map((h) => h.commit.oid)).size;
}

export const MATCH_LABEL: Record<GitHistoryField, string> = {
  message: "Message",
  author: "Author",
  path: "Path",
  content: "Content",
};

export interface SearchFields {
  message: string;
  author: string;
  path: string;
  content: string;
  /** `YYYY-MM-DD` or empty. */
  since: string;
  until: string;
}

export const EMPTY_FIELDS: SearchFields = { message: "", author: "", path: "", content: "", since: "", until: "" };

export function fieldsOf(query: Partial<GitHistoryQuery> | undefined): SearchFields {
  const day = (t: number | undefined) => (t === undefined ? "" : new Date(t * 1000).toISOString().slice(0, 10));
  return {
    message: query?.message ?? "",
    author: query?.author ?? "",
    path: query?.path ?? "",
    content: query?.content ?? "",
    since: day(query?.since),
    until: day(query?.until),
  };
}

/** The query for the fields, without the paging; `null` when nothing is asked. `since` is the
 *  start of its day and `until` the end of its day (UTC). */
export function toQuery(fields: SearchFields): Omit<GitHistoryQuery, "limit" | "skip"> | null {
  const text = (s: string) => s.trim();
  const out: Omit<GitHistoryQuery, "limit" | "skip"> = {};
  if (text(fields.message)) out.message = text(fields.message);
  if (text(fields.author)) out.author = text(fields.author);
  if (text(fields.path)) out.path = text(fields.path).startsWith("/") ? text(fields.path) : `/${text(fields.path)}`;
  if (text(fields.content)) out.content = text(fields.content);
  const since = Date.parse(`${fields.since}T00:00:00Z`);
  const until = Date.parse(`${fields.until}T23:59:59Z`);
  if (fields.since && !Number.isNaN(since)) out.since = Math.floor(since / 1000);
  if (fields.until && !Number.isNaN(until)) out.until = Math.floor(until / 1000);
  return Object.keys(out).length > 0 ? out : null;
}

/** A name for the branch that brings a lost commit back. */
export function recoveredName(entry: Pick<GitReflogEntry, "new">): string {
  return `recovered-${entry.new.slice(0, 7)}`;
}

export interface ReflogRow {
  entry: GitReflogEntry;
  /** No branch, tag or remote branch reaches the commit: it is only in the reflog. `null`
   *  when that is not known (the history is longer than what was looked at). */
  lost: boolean | null;
}

/** Marks the entries whose commit is not in `reachable` (every commit the graph shows). When
 *  the graph has more pages than were loaded, a commit that is not in them is unknown, not
 *  lost. */
export function reflogRows(entries: readonly GitReflogEntry[], reachable: ReadonlySet<string>, complete: boolean): ReflogRow[] {
  return entries.map((entry) => ({ entry, lost: reachable.has(entry.new) ? false : complete ? true : null }));
}

/** The commit of a search hit as the graph's row, for navigation. */
export function hitTarget(commit: GitGraphCommit): string {
  return commit.oid;
}

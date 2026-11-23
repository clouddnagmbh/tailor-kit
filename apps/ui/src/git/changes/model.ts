// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// The "Changes" view of the working copy (wave 15, strand B) without a DOM: which files a
// filter keeps, how they are grouped by folder, the counts per state, and how a row is shown.

import type { GitFileState, GitWorkingFile } from "../../commands/types";

export type StateFilter = "all" | GitFileState;

export interface ChangeFilter {
  text: string;
  state: StateFilter;
  /** Only files with something in the index. */
  staged: boolean;
}

export const NO_FILTER: ChangeFilter = { text: "", state: "all", staged: false };

/** Rows of a rename: the new file names the old one, and the old one's own row is folded in. */
export function visibleFiles(files: GitWorkingFile[]): GitWorkingFile[] {
  const renamedFrom = new Set(files.flatMap((f) => (f.from ? [f.from] : [])));
  return files.filter((f) => !(f.state === "deleted" && renamedFrom.has(f.path)));
}

/** The files a rename covers: itself and the old path, so a commit takes both. */
export function pathsOf(file: GitWorkingFile): string[] {
  return file.from ? [file.path, file.from] : [file.path];
}

export function filterFiles(files: GitWorkingFile[], filter: ChangeFilter): GitWorkingFile[] {
  const needle = filter.text.trim().toLowerCase();
  return files.filter(
    (f) =>
      (filter.state === "all" || f.state === filter.state) &&
      (!filter.staged || f.staged !== undefined) &&
      (needle === "" || f.path.toLowerCase().includes(needle) || (f.from?.toLowerCase().includes(needle) ?? false)),
  );
}

export interface StateCounts {
  all: number;
  untracked: number;
  modified: number;
  deleted: number;
  conflicted: number;
}

export function countStates(files: GitWorkingFile[]): StateCounts {
  const counts: StateCounts = { all: files.length, untracked: 0, modified: 0, deleted: 0, conflicted: 0 };
  for (const f of files) counts[f.state]++;
  return counts;
}

export interface FolderGroup {
  /** `/a/b` or `/` for files at the top. */
  folder: string;
  files: GitWorkingFile[];
}

/** The folder of a repository path: `/a/b/c.yml` → `/a/b`, `/c.yml` → `/`. */
export function folderOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at <= 0 ? "/" : path.slice(0, at);
}

export function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** Files grouped by folder, folders and files in path order. */
export function groupByFolder(files: GitWorkingFile[]): FolderGroup[] {
  const groups = new Map<string, GitWorkingFile[]>();
  for (const f of files) {
    const folder = folderOf(f.path);
    const list = groups.get(folder);
    if (list) list.push(f);
    else groups.set(folder, [f]);
  }
  return [...groups]
    .sort(([a], [b]) => (a === "/" ? -1 : b === "/" ? 1 : a < b ? -1 : a > b ? 1 : 0))
    .map(([folder, list]) => ({ folder, files: list.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0)) }));
}

/** Status letter of the Badge: `A` new, `M` modified, `D` deleted, `R` renamed, `U` conflict. */
export function statusLetter(file: GitWorkingFile): "A" | "M" | "D" | "R" | "U" | "?" {
  if (file.state === "conflicted") return "U";
  if (file.from) return "R";
  if (file.state === "deleted") return "D";
  if (file.state === "modified") return "M";
  return file.staged === "added" ? "A" : "?";
}

/** One line for the header: "3 modified, 1 new, 2 deleted". */
export function summaryText(counts: StateCounts): string {
  const parts: string[] = [];
  if (counts.modified) parts.push(`${counts.modified} modified`);
  if (counts.untracked) parts.push(`${counts.untracked} new`);
  if (counts.deleted) parts.push(`${counts.deleted} deleted`);
  if (counts.conflicted) parts.push(`${counts.conflicted} in conflict`);
  return parts.length === 0 ? "No changes" : parts.join(", ");
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand G: what the Git panel shows, as pure functions (unit-tested, the same in every
// shell). The panel component only renders these: the situation of the repository in plain
// words, the one contextual sync action, the changed files with their badges, the tool tiles and
// the lanes of the recent commits.

import type { Command } from "../../shell/commands";
import type { GitBranch, GitFile, GitFileState, GitGraphCommit, GitRef, GitStatus, TreeNode } from "../../commands/types";
import type { GitTerm } from "../glossary";

/** The one situation the panel explains at the top. Order of precedence as listed. */
export type GitSituation =
  | { kind: "loading" }
  | { kind: "no-repo" }
  /** A repository without commits yet. */
  | { kind: "empty"; branch: string }
  /** Pull stopped with conflicts (some files still conflicted). */
  | { kind: "conflicts"; files: number }
  /** Conflicts resolved; a commit finishes the merge. */
  | { kind: "merging" }
  | { kind: "detached" }
  | { kind: "no-remote"; branch: string }
  /** A remote exists but this branch is not on it. */
  | { kind: "unpublished"; branch: string }
  | { kind: "diverged"; branch: string; upstream: string; ahead: number; behind: number }
  | { kind: "behind"; branch: string; upstream: string; behind: number }
  | { kind: "ahead"; branch: string; upstream: string; ahead: number }
  | { kind: "synced"; branch: string; upstream: string };

/**
 * `remotes`: number of remotes, `null` when unknown (a shell that cannot list them yet). With no
 * remote at all a branch without upstream needs "Add a remote" first; otherwise "Publish".
 */
export function situation(
  status: GitStatus | null,
  branches: GitBranch[],
  hasCommits: boolean,
  remotes: number | null,
): GitSituation {
  if (!status) return { kind: "loading" };
  if (!status.repo) return { kind: "no-repo" };
  if (!hasCommits) {
    const current = status.branch ?? branches.find((b) => b.current && !b.remote)?.name ?? "main";
    return { kind: "empty", branch: current };
  }
  const conflicted = status.files.filter((f) => f.state === "conflicted").length;
  if (conflicted > 0) return { kind: "conflicts", files: conflicted };
  if (status.merging) return { kind: "merging" };
  const branch = status.branch;
  if (!branch) return { kind: "detached" };
  const upstream = status.upstream;
  if (!upstream) return remotes === 0 ? { kind: "no-remote", branch } : { kind: "unpublished", branch };
  const { ahead, behind } = status;
  if (ahead > 0 && behind > 0) return { kind: "diverged", branch, upstream, ahead, behind };
  if (behind > 0) return { kind: "behind", branch, upstream, behind };
  if (ahead > 0) return { kind: "ahead", branch, upstream, ahead };
  return { kind: "synced", branch, upstream };
}

/** What the sync line says, with the glossary term its tooltip explains. */
export interface SyncState {
  /** Short text next to the icons: "↑1 ↓2", "Up to date", "Not published". */
  text: string;
  ahead: number;
  behind: number;
  /** Full sentence for the tooltip and screen readers. */
  title: string;
  term: GitTerm;
  tone: "neutral" | "ok" | "attention" | "warn";
}

export function syncState(s: GitSituation): SyncState | null {
  switch (s.kind) {
    case "synced":
      return { text: "Up to date", ahead: 0, behind: 0, tone: "ok", term: "upstream", title: `In sync with ${s.upstream}: nothing to send or get.` };
    case "ahead":
      return {
        text: `↑${s.ahead}`,
        ahead: s.ahead,
        behind: 0,
        tone: "attention",
        term: "ahead",
        title: `${count(s.ahead, "commit")} to send to ${s.upstream}.`,
      };
    case "behind":
      return {
        text: `↓${s.behind}`,
        ahead: 0,
        behind: s.behind,
        tone: "attention",
        term: "behind",
        title: `${count(s.behind, "commit")} to get from ${s.upstream}.`,
      };
    case "diverged":
      return {
        text: `↑${s.ahead} ↓${s.behind}`,
        ahead: s.ahead,
        behind: s.behind,
        tone: "warn",
        term: "diverged",
        title: `You have ${count(s.ahead, "commit")} ${s.upstream} lacks, and it has ${count(s.behind, "commit")} you lack.`,
      };
    case "unpublished":
      return { text: "Not published", ahead: 0, behind: 0, tone: "neutral", term: "publish", title: `${s.branch} is only on this computer so far.` };
    case "no-remote":
      return { text: "No remote", ahead: 0, behind: 0, tone: "neutral", term: "remote", title: "This repository is not linked to a server yet." };
    case "detached":
      return { text: "Not on a branch", ahead: 0, behind: 0, tone: "warn", term: "detached", title: "You are looking at an older commit, not at a branch." };
    default:
      return null;
  }
}

/** The single contextual action of the header. `wizard` opens a Git wizard instead of running. */
export type SyncActionKind = "pull" | "push" | "sync" | "publish" | "add-remote" | "fetch" | "create-branch-here";

export interface SyncAction {
  kind: SyncActionKind;
  label: string;
  /** Primary (accent) or quiet (when nothing is waiting). */
  primary: boolean;
  title: string;
}

export function syncAction(s: GitSituation): SyncAction | null {
  switch (s.kind) {
    case "behind":
      return { kind: "pull", label: `Pull ${s.behind}`, primary: true, title: `Get ${count(s.behind, "commit")} from ${s.upstream} and merge them in.` };
    case "ahead":
      return { kind: "push", label: `Push ${s.ahead}`, primary: true, title: `Send ${count(s.ahead, "commit")} to ${s.upstream}.` };
    case "diverged":
      return { kind: "sync", label: "Sync…", primary: true, title: "Both sides have new commits. Choose how to bring them together." };
    case "unpublished":
      return { kind: "publish", label: "Publish branch", primary: true, title: `Put ${s.branch} on the server and link the two.` };
    case "no-remote":
      return { kind: "add-remote", label: "Add a remote…", primary: true, title: "Link this repository to a server (for example GitHub) to share it." };
    case "synced":
      return { kind: "fetch", label: "Check for changes", primary: false, title: `Ask ${s.upstream} whether anything new arrived.` };
    case "detached":
      return { kind: "create-branch-here", label: "Create a branch here", primary: true, title: "Keep working from this commit on a new branch." };
    default:
      return null;
  }
}

/** A changed file as the list shows it. */
export interface ChangeRow {
  path: string;
  state: GitFileState;
  /** A, M, D, C (R once the status reports renames). */
  badge: "A" | "M" | "D" | "U" | "R";
  /** Plain words for the badge. */
  badgeTitle: string;
  /** The request's name, or the file name for any other file. */
  title: string;
  /** Folder part of the path ("" at the root). */
  dir: string;
  /** HTTP method etc. when the file is a request of the collection. */
  method: string | null;
}

const BADGE: Record<GitFileState, ChangeRow["badge"]> = { untracked: "A", modified: "M", deleted: "D", conflicted: "U" };
const BADGE_TITLE: Record<GitFileState, string> = {
  untracked: "Added: a new file",
  modified: "Modified",
  deleted: "Deleted",
  conflicted: "Conflict: choose which version to keep",
};

function requests(tree: TreeNode | null, out = new Map<string, { name: string; method: string }>()) {
  if (!tree) return out;
  if (tree.kind === "request") out.set(tree.path, { name: tree.name, method: tree.method });
  else for (const c of tree.children) requests(c, out);
  return out;
}

export function changeRows(files: GitFile[], tree: TreeNode | null): ChangeRow[] {
  const byPath = requests(tree);
  return files.map((f) => {
    const slash = f.path.lastIndexOf("/");
    const req = byPath.get(f.path);
    return {
      path: f.path,
      state: f.state,
      badge: BADGE[f.state],
      badgeTitle: BADGE_TITLE[f.state],
      title: req?.name ?? f.path.slice(slash + 1),
      dir: slash >= 0 ? f.path.slice(0, slash) : "",
      method: req?.method ?? null,
    };
  });
}

/** "Commit 3 changes". */
export function commitLabel(n: number): string {
  return n === 0 ? "Commit" : `Commit ${count(n, "change")}`;
}

/** A tile of the tools grid. */
export interface ToolTile {
  id: string;
  label: string;
  /** Icon name (resolved by the panel). */
  icon: string;
  /** The full explanation (tooltip). */
  tip: string;
  run: () => unknown;
}

/** Short labels and explanations for the wave-12 tools by palette id; unknown ids keep their
 *  palette label. Order is the grid's. */
const TOOLS: { id: string; label: string; icon: string; tip: string }[] = [
  { id: "git:compare", label: "Compare", icon: "compare", tip: "Compare two branches, commits or tags: what changed in the requests and how their responses differ." },
  { id: "timeline-request", label: "History", icon: "history", tip: "History of the open request: every version, who changed which field, and old test results." },
  { id: "git:bisect", label: "Bisect", icon: "bisect", tip: "Find the commit that broke a test by running it on older versions, halving the range each time." },
  { id: "git:review", label: "Review", icon: "review", tip: "Review pull and merge requests from GitHub or GitLab: changes by field, contract notes and checks." },
  { id: "git:review-create", label: "New PR", icon: "review", tip: "Open a pull or merge request from a branch on GitHub or GitLab: it asks you to publish the branch first if it is not on the server." },
  { id: "contract:open", label: "Contract", icon: "contract", tip: "Check the requests against the OpenAPI spec and list breaking changes between versions." },
  { id: "git:rescue", label: "Secrets", icon: "shield-secret", tip: "Scan the whole history for passwords and tokens that were committed, and move them out safely." },
];

/** Palette entries that the panel shows elsewhere, not as tiles. */
const NOT_TILES = new Set(["diff-working", "diff-head", "merge-conflicts"]);

export function toolTiles(commands: Command[], openGraph: () => void): ToolTile[] {
  const byId = new Map(commands.map((c) => [c.id, c]));
  const tiles: ToolTile[] = [
    { id: "graph", label: "Graph", icon: "graph", tip: "All branches and commits of the repository as a graph, like Fork or Tower.", run: openGraph },
  ];
  for (const t of TOOLS) {
    const c = byId.get(t.id);
    if (c) tiles.push({ ...t, run: c.run });
  }
  for (const c of commands) {
    if (NOT_TILES.has(c.id) || c.id.startsWith("git-wizard:") || TOOLS.some((t) => t.id === c.id)) continue;
    tiles.push({ id: c.id, label: c.label.replace(/[:…(].*$/, "").trim() || c.label, icon: "settings", tip: c.label, run: c.run });
  }
  return tiles;
}

/** A ref label on a commit row. */
export interface RefPill {
  name: string;
  kind: "local" | "remote" | "tag" | "head";
  current: boolean;
}

export function refPills(oid: string, refs: GitRef[]): RefPill[] {
  const order = { head: 0, local: 1, remote: 2, tag: 3 } as const;
  return refs
    .filter((r) => r.target === oid && !(r.kind === "remote" && r.name.endsWith("/HEAD")))
    .map((r) => ({ name: r.name, kind: r.kind, current: r.current }))
    .sort((a, b) => order[a.kind] - order[b.kind] || Number(b.current) - Number(a.current) || a.name.localeCompare(b.name));
}

/** A row of the mini lane graph: the commit's lane and the lanes passing it. */
export interface LaneRow {
  oid: string;
  lane: number;
  /** Lanes that continue from the row above into this row (before the commit). */
  above: number[];
  /** Lanes that continue below this row. */
  below: number[];
  /** Lanes this commit's parents are on, other than its own (merge and branch edges). */
  joins: number[];
  merge: boolean;
}

/**
 * A small lane layout for the panel's few recent commits (the graph view has the full one).
 * Each lane waits for one commit id; a commit takes the leftmost lane waiting for it, its first
 * parent continues in that lane, other parents take free lanes. Lanes whose parent lies outside
 * the shown commits just run off the bottom.
 */
export function miniLanes(commits: Pick<GitGraphCommit, "oid" | "parents">[]): LaneRow[] {
  const lanes: (string | null)[] = [];
  const rows: LaneRow[] = [];
  for (const c of commits) {
    const above = lanes.flatMap((o, i) => (o ? [i] : []));
    let lane = lanes.indexOf(c.oid);
    if (lane < 0) {
      lane = lanes.indexOf(null);
      if (lane < 0) lane = lanes.length;
    }
    // Other lanes waiting for the same commit end here.
    const joins: number[] = [];
    for (let i = 0; i < lanes.length; i++) {
      if (i !== lane && lanes[i] === c.oid) {
        lanes[i] = null;
        joins.push(i);
      }
    }
    lanes[lane] = c.parents[0] ?? null;
    for (const p of c.parents.slice(1)) {
      let at = lanes.indexOf(p);
      if (at < 0) {
        at = lanes.indexOf(null);
        if (at < 0) at = lanes.length;
        lanes[at] = p;
      }
      joins.push(at);
    }
    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
    rows.push({
      oid: c.oid,
      lane,
      above,
      below: lanes.flatMap((o, i) => (o ? [i] : [])),
      joins: [...new Set(joins)].sort((a, b) => a - b),
      merge: c.parents.length > 1,
    });
  }
  return rows;
}

/** Up to two initials of an author ("Ada Lovelace" → "AL"). */
export function initials(name: string): string {
  const parts = name.trim().split(/[\s._-]+/).filter(Boolean);
  if (parts.length === 0) return "?";
  const first = parts[0][0] ?? "";
  const last = parts.length > 1 ? (parts[parts.length - 1][0] ?? "") : (parts[0][1] ?? "");
  return (first + last).toUpperCase();
}

/** A stable lane-colour index (1..8) for an author, so a person keeps their colour. */
export function authorHue(name: string): number {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return (h % 8) + 1;
}

/** Compact relative time: "now", "5m", "3h", "2d", then the date. */
export function shortAgo(unixSeconds: number, nowMs = Date.now()): string {
  const s = Math.max(0, Math.round(nowMs / 1000 - unixSeconds));
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  if (s < 30 * 86400) return `${Math.floor(s / 86400)}d`;
  return new Date(unixSeconds * 1000).toISOString().slice(0, 10);
}

/** Repository name for the header: the last path segment of the remote URL or of the folder. */
export function repoName(remoteUrl: string | null, location: string | null): string {
  const from = (s: string) =>
    s
      .replace(/[?#].*$/, "")
      .replace(/\/+$/, "")
      .replace(/\.git$/, "")
      .split(/[/\\:]/)
      .filter(Boolean)
      .pop() ?? "";
  return (remoteUrl && from(remoteUrl)) || (location && from(location)) || "Repository";
}

function count(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: what the commit graph shows, as pure functions (tested in model.test.ts):
// the ref labels of a commit (a remote branch in step with its local branch folds into it),
// dates, search, and the actions of the context menus with the wizard presets of strand G
// (`../wizards/open.ts`) and the views of strand H (`compare`, `diff`).

import type { CollectionDiff, GitGraphCommit, GitRef } from "../types";
import type { GitView } from "../views.svelte";
import type { GitWizardRequest } from "../wizards/open";

/** A label on a commit row: a ref, and the remote branch folded into it when both point at
 *  the same commit (`main` + `origin/main` show as one label). */
export interface RefLabel {
  ref: GitRef;
  tracking?: GitRef;
}

const ORDER: Record<GitRef["kind"], number> = { head: 0, local: 1, remote: 2, tag: 3 };

/** The labels of the refs `full` (one commit's), in the order people read them: a detached
 *  HEAD, the current branch, other branches, remote branches, tags. */
export function refLabels(full: readonly string[] | undefined, byFull: ReadonlyMap<string, GitRef>): RefLabel[] {
  if (!full || full.length === 0) return [];
  const refs = full.map((f) => byFull.get(f)).filter((r): r is GitRef => r !== undefined);
  const folded = new Set<string>();
  const labels: RefLabel[] = [];
  for (const r of refs) {
    if (r.kind !== "local") continue;
    const tracking = r.upstream ? refs.find((x) => x.kind === "remote" && x.name === r.upstream && x.target === r.target) : undefined;
    if (tracking) folded.add(tracking.full);
    labels.push(tracking ? { ref: r, tracking } : { ref: r });
  }
  for (const r of refs) if (r.kind !== "local" && !folded.has(r.full)) labels.push({ ref: r });
  return labels.sort((a, b) => ORDER[a.ref.kind] - ORDER[b.ref.kind] || Number(b.ref.current) - Number(a.ref.current) || (a.ref.name < b.ref.name ? -1 : 1));
}

/** First seven characters of a commit id, as Git shows it. */
export const shortId = (oid: string) => oid.slice(0, 7);

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** "just now", "5 minutes ago", "3 hours ago", "yesterday", "4 days ago", "Nov 3", "Nov 3, 2024". */
export function relativeTime(unixSeconds: number, nowMs = Date.now()): string {
  const s = Math.round(nowMs / 1000 - unixSeconds);
  if (s < 0) return dateOnly(unixSeconds, nowMs);
  if (s < 60) return "just now";
  if (s < 3_600) return `${plural(Math.floor(s / 60), "minute")} ago`;
  if (s < 86_400) return `${plural(Math.floor(s / 3_600), "hour")} ago`;
  const days = Math.floor(s / 86_400);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  return dateOnly(unixSeconds, nowMs);
}

// `toLocale*String(locale, options)` builds a new Intl formatter on every call, which cost more
// than everything else while scrolling (two calls per row). The formatters are made once.
const SAME_YEAR = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" });
const OTHER_YEAR = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric" });
const FULL = new Intl.DateTimeFormat("en-US", { weekday: "short", year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

function dateOnly(unixSeconds: number, nowMs: number): string {
  const d = new Date(unixSeconds * 1000);
  return (d.getFullYear() === new Date(nowMs).getFullYear() ? SAME_YEAR : OTHER_YEAR).format(d);
}

/** The full date and time, for tooltips and the details. */
export function absoluteTime(unixSeconds: number): string {
  return FULL.format(new Date(unixSeconds * 1000));
}

/** Whether `commit` matches the search `query`: words of the message, the author, an id
 *  prefix or a ref name (case-insensitive). */
export function matches(commit: GitGraphCommit, byFull: ReadonlyMap<string, GitRef>, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return false;
  if (/^[0-9a-f]{4,40}$/.test(q) && commit.oid.startsWith(q)) return true;
  if (commit.summary.toLowerCase().includes(q)) return true;
  if (commit.author.toLowerCase().includes(q) || commit.email.toLowerCase().includes(q)) return true;
  return (commit.refs ?? []).some((f) => byFull.get(f)?.name.toLowerCase().includes(q));
}

/** What a menu entry does. */
export type GraphAction =
  | { kind: "wizard"; request: GitWizardRequest }
  | { kind: "view"; view: GitView }
  | { kind: "copy"; text: string }
  | { kind: "select"; rev: string }
  /** Wave 15: "Create pull request…" from the branch `head`. */
  | { kind: "pull-request"; head: string };

export interface GraphMenuItem {
  id: string;
  label: string;
  icon?: string;
  hint?: string;
  danger?: boolean;
  disabled?: boolean;
  action?: GraphAction;
  /** A submenu (the refs at a commit). */
  items?: GraphMenuEntry[];
}

export type GraphMenuEntry = GraphMenuItem | { separator: true };

/** Where the repository stands, for what the menus offer. */
export interface GraphContext {
  /** The commit `HEAD` points to. */
  head: string | null;
  /** The current branch (short name); `null` while detached. */
  branch: string | null;
}

const wizard = (kind: GitWizardRequest["kind"], preset: Record<string, unknown>): GraphAction => ({ kind: "wizard", request: { kind, preset } });
const SEP = { separator: true } as const;

/** The menu of a commit row: start something here, take it over, compare, and one submenu per
 *  branch or tag at it. */
export function commitMenu(commit: GitGraphCommit, labels: readonly RefLabel[], ctx: GraphContext): GraphMenuEntry[] {
  const oid = commit.oid;
  const isHead = oid === ctx.head;
  const short = shortId(oid);
  const entries: GraphMenuEntry[] = [
    { id: "branch-here", label: "Branch from here…", icon: "branch", action: wizard("create-branch", { start: oid }) },
    { id: "tag-here", label: "Tag this commit…", icon: "tag", action: wizard("create-tag", { target: oid }) },
    SEP,
    {
      id: "cherry-pick",
      label: ctx.branch ? `Copy this commit to ${ctx.branch}` : "Copy this commit here",
      icon: "copy",
      disabled: isHead,
      hint: isHead ? "Already here" : undefined,
      action: wizard("cherry-pick", { commit: oid }),
    },
    { id: "revert", label: "Undo this commit…", icon: "undo", action: wizard("revert", { commit: oid }) },
    {
      id: "reset",
      label: ctx.branch ? `Move ${ctx.branch} here…` : "Move branch here…",
      icon: "history",
      disabled: isHead || !ctx.branch,
      hint: !ctx.branch ? "Not on a branch" : undefined,
      action: wizard("reset", { target: oid }),
    },
    SEP,
    { id: "compare-current", label: "Compare with current", icon: "compare", disabled: isHead, action: { kind: "view", view: { kind: "compare", from: oid, to: "HEAD" } } },
    { id: "compare", label: "Compare with…", icon: "compare", action: { kind: "view", view: { kind: "compare", from: oid } } },
    { id: "request-changes", label: "Show request changes", icon: "file-text", action: { kind: "view", view: { kind: "diff", scope: { kind: "commit", rev: oid } } } },
    SEP,
    { id: "copy-id", label: `Copy commit id (${short})`, icon: "copy", action: { kind: "copy", text: oid } },
  ];
  const refs = labels.filter((l) => l.ref.kind !== "head");
  if (refs.length > 0) {
    entries.push(SEP);
    for (const l of refs) {
      entries.push({ id: `ref:${l.ref.full}`, label: l.ref.name, icon: l.ref.kind === "tag" ? "tag" : "branch", items: refMenu(l.ref, ctx) });
    }
  }
  const head = labels.find((l) => l.ref.kind === "head");
  if (head) {
    entries.unshift({ id: "detached", label: "Create a branch here (you are not on one)…", icon: "branch", action: wizard("detached", {}) }, SEP);
  }
  return entries;
}

/** The menu of a branch, remote branch or tag label. */
export function refMenu(ref: GitRef, ctx: GraphContext): GraphMenuEntry[] {
  const name = ref.name;
  const compare: GraphMenuEntry = { id: "compare-current", label: "Compare with current", icon: "compare", disabled: ref.current || ref.target === ctx.head, action: { kind: "view", view: { kind: "compare", from: "HEAD", to: name } } };
  const into = ctx.branch ? `Merge into ${ctx.branch}…` : "Merge into current…";
  switch (ref.kind) {
    case "local":
      return [
        { id: "switch", label: `Switch to ${name}`, icon: "branch", disabled: ref.current, hint: ref.current ? "Current branch" : undefined, action: wizard("switch", { target: name }) },
        { id: "merge", label: into, icon: "merge", disabled: ref.current || !ctx.branch, action: wizard("merge", { from: name }) },
        ...(ref.upstream ? [] : [{ id: "publish", label: "Publish branch…", icon: "push", action: wizard("publish", { name }) } as GraphMenuEntry]),
        { id: "pull-request", label: "Create pull request…", icon: "review", action: { kind: "pull-request", head: name } },
        SEP,
        { id: "rename", label: "Rename…", icon: "edit", action: wizard("rename-branch", { from: name }) },
        { id: "delete", label: "Delete…", icon: "delete", danger: true, disabled: ref.current, hint: ref.current ? "Current branch" : undefined, action: wizard("delete-branch", { name }) },
        SEP,
        compare,
        { id: "copy-name", label: "Copy branch name", icon: "copy", action: { kind: "copy", text: name } },
      ];
    case "remote":
      return [
        { id: "switch", label: `Switch to ${name}`, icon: "branch", hint: "Makes a local branch", action: wizard("switch", { target: name }) },
        { id: "merge", label: into, icon: "merge", disabled: !ctx.branch, action: wizard("merge", { from: name }) },
        SEP,
        compare,
        { id: "copy-name", label: "Copy branch name", icon: "copy", action: { kind: "copy", text: name } },
      ];
    case "tag":
      return [
        { id: "branch-here", label: "Branch from this tag…", icon: "branch", action: wizard("create-branch", { start: ref.target }) },
        { id: "push-tag", label: "Publish tag…", icon: "push", action: wizard("push-tag", { name }) },
        { id: "delete-tag", label: "Delete tag…", icon: "delete", danger: true, action: wizard("delete-tag", { name }) },
        SEP,
        compare,
        { id: "copy-name", label: "Copy tag name", icon: "copy", action: { kind: "copy", text: name } },
      ];
    case "head":
      return [
        { id: "detached", label: "Create a branch here…", icon: "branch", action: wizard("detached", {}) },
        { id: "copy-id", label: `Copy commit id (${shortId(ref.target)})`, icon: "copy", action: { kind: "copy", text: ref.target } },
      ];
  }
}

/** Where a selection request (`GitView` `graph` `select`: a commit id, an id prefix or a full
 *  or short ref name) points, among the refs; `null` if it names no ref. */
export function selectTarget(select: string, refs: readonly GitRef[]): string | null {
  const r = refs.find((x) => x.full === select || x.name === select);
  return r ? r.target : null;
}

/** How the "to send"/"to get" counts of a branch read, e.g. "2 to send, 1 to get (origin/main)". */
export function syncText(ref: GitRef): string {
  if (!ref.upstream) return "Not published yet";
  const parts = [ref.ahead > 0 ? `${ref.ahead} to send` : "", ref.behind > 0 ? `${ref.behind} to get` : ""].filter(Boolean);
  return parts.length > 0 ? `${parts.join(", ")} (${ref.upstream})` : `In step with ${ref.upstream}`;
}

/** The part of a collection's semantic diff that is about the repository file `path` (leading
 *  `/`; the diff's files are relative to the collection, which may sit in a folder of the
 *  repository): the longest collection path that `path` ends with. */
export function semanticFor(diff: CollectionDiff, path: string): CollectionDiff {
  const fits = (f: string) => path === `/${f}` || path.endsWith(`/${f}`);
  let file: string | null = null;
  for (const f of [...diff.changes.map((c) => c.file), ...diff.files.map((x) => x.path)]) {
    if (fits(f) && (file === null || f.length > file.length)) file = f;
  }
  if (file === null) return { changes: [], files: [] };
  return { changes: diff.changes.filter((c) => c.file === file), files: diff.files.filter((f) => f.path === file) };
}

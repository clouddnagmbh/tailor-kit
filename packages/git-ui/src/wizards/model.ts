// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand G: what each Git wizard asks and which `GitAction` it builds, as pure
// functions (unit-tested). `GitWizard.svelte` renders the fields, plans while the user types,
// shows the plan and applies on confirmation.

import type { GitAction, GitBlockReason, GitPlan, GitRef, GitResetMode } from "../types";
import type { GitWizardKind } from "./open";
import type { GitTerm } from "../glossary";
import { refNameProblem } from "./refname";

/** What the user filled in; every wizard uses the fields it needs. */
export interface WizardValues {
  name: string;
  /** Branch or rev: start of a new branch, target of switch/reset/tag, what to merge. */
  target: string;
  from: string;
  url: string;
  remote: string;
  message: string;
  commit: string;
  mode: GitResetMode;
  switchTo: boolean;
  force: boolean;
  keep: boolean;
  index: number;
  paths: string[];
  /** The way chosen in "diverged" (`merge` | `theirs`) and "detached" (`branch` | `back`). */
  choice: string;
}

/** What the wizard knows of the repository when it opens. */
export interface WizardContext {
  current: string | null;
  upstream: string | null;
  refs: GitRef[];
}

export function initialValues(kind: GitWizardKind, preset: Record<string, unknown> = {}, ctx: WizardContext): WizardValues {
  const str = (k: string, fallback = "") => (typeof preset[k] === "string" ? (preset[k] as string) : fallback);
  const locals = ctx.refs.filter((r) => r.kind === "local" && !r.current);
  const values: WizardValues = {
    // `branch` is accepted as an alias of `name` (publish, delete-branch from the graph).
    name: str("name", str("branch")),
    target: str("target", str("start", str(kind === "restore-files" ? "rev" : "tip", kind === "create-branch" || kind === "create-tag" || kind === "restore-files" ? "HEAD" : ""))),
    from: str("from", kind === "rename-branch" ? (ctx.current ?? "") : ""),
    url: str("url"),
    remote: str("remote", "origin"),
    message: str("message", str("summary")),
    commit: str("commit"),
    mode: preset.mode === "discard" ? "discard" : "keep-changes",
    switchTo: preset.switch !== false,
    force: preset.force === true,
    keep: preset.keep === true,
    index: typeof preset.index === "number" ? preset.index : 0,
    paths: Array.isArray(preset.paths) ? (preset.paths as string[]) : [],
    choice: str("choice", kind === "diverged" ? "merge" : kind === "detached" ? "branch" : ""),
  };
  if (kind === "merge" && !values.from) values.from = locals.find((r) => r.name === "main")?.name ?? locals[0]?.name ?? "";
  if (kind === "delete-branch" && !values.name) values.name = locals[0]?.name ?? "";
  if (kind === "switch" && !values.target) values.target = locals[0]?.name ?? "";
  if (kind === "detached" && !values.target) values.target = ctx.refs.find((r) => r.kind === "local" && r.name === "main")?.name ?? locals[0]?.name ?? "";
  if (kind === "publish" && !values.name) values.name = ctx.current ?? "";
  if (kind === "reset" && !values.target) values.target = "HEAD~1";
  return values;
}

/** The action the values ask for; `null` while something required is missing. */
export function toAction(kind: GitWizardKind, v: WizardValues, ctx: WizardContext): GitAction | null {
  const name = v.name.trim();
  switch (kind) {
    case "create-branch":
      return name ? { action: "create-branch", name, start: v.target || "HEAD", switch: v.switchTo } : null;
    case "switch":
      return v.target ? { action: "switch", target: v.target } : null;
    case "rename-branch":
      return v.from && name ? { action: "rename-branch", from: v.from, to: name } : null;
    case "delete-branch":
      return name ? { action: "delete-branch", name, force: v.force } : null;
    case "merge":
      return v.from ? { action: "merge", from: v.from } : null;
    case "pull":
      return { action: "pull" };
    case "push":
      return { action: "push" };
    case "publish":
      return name && v.remote ? { action: "publish", branch: name, remote: v.remote } : null;
    case "fetch":
      return { action: "fetch" };
    case "add-remote":
      return name && v.url.trim() ? { action: "add-remote", name, url: v.url.trim() } : null;
    case "remove-remote":
      return v.remote ? { action: "remove-remote", name: v.remote } : null;
    case "rename-remote":
      return v.remote && name ? { action: "rename-remote", from: v.remote, to: name } : null;
    case "set-remote-url":
      return v.remote && v.url.trim() ? { action: "set-remote-url", name: v.remote, url: v.url.trim() } : null;
    case "stash":
      return { action: "stash", ...(v.message.trim() ? { message: v.message.trim() } : {}), ...(v.paths.length > 0 ? { paths: v.paths } : {}) };
    case "amend": {
      const message = v.message.trim();
      return message || v.paths.length > 0 ? { action: "amend", ...(message ? { message } : {}), ...(v.paths.length > 0 ? { paths: v.paths } : {}) } : null;
    }
    case "abort":
      return { action: "abort" };
    case "restore-files":
      return v.paths.length > 0 ? { action: "restore-files", rev: v.target || "HEAD", paths: v.paths } : null;
    case "restore-branch":
      return name && v.target ? { action: "restore-branch", name, target: v.target } : null;
    case "rewrite":
      return name && v.target ? { action: "rewrite", branch: name, tip: v.target, summary: v.message.trim() } : null;
    case "unstash":
      return { action: "unstash", index: v.index, keep: v.keep };
    case "drop-stash":
      return { action: "drop-stash", index: v.index };
    case "revert":
      return v.commit ? { action: "revert", commit: v.commit } : null;
    case "cherry-pick":
      return v.commit ? { action: "cherry-pick", commit: v.commit } : null;
    case "reset":
      return v.target ? { action: "reset", target: v.target, mode: v.mode } : null;
    case "discard":
      return { action: "discard", paths: v.paths };
    case "create-tag":
      return name ? { action: "create-tag", name, target: v.target || "HEAD", message: v.message.trim() || undefined } : null;
    case "delete-tag":
      return name ? { action: "delete-tag", name } : null;
    case "push-tag":
      return name && v.remote ? { action: "push-tag", name, remote: v.remote } : null;
    case "diverged":
      return v.choice === "theirs" && ctx.upstream ? { action: "reset", target: ctx.upstream, mode: "discard" } : { action: "pull" };
    case "detached":
      return v.choice === "back"
        ? v.target
          ? { action: "switch", target: v.target }
          : null
        : name
          ? { action: "create-branch", name, start: "HEAD", switch: true }
          : null;
    default:
      return null;
  }
}

/** A name field's problem while typing (before asking the backend). */
export function nameProblem(kind: GitWizardKind, v: WizardValues): string | null {
  const named = ["create-branch", "rename-branch", "create-tag", "add-remote", "rename-remote", "restore-branch"].includes(kind) || (kind === "detached" && v.choice === "branch");
  if (!named || v.name.trim() === "") return null;
  return refNameProblem(v.name.trim());
}

/** The confirm button: says what happens. */
export function confirmLabel(kind: GitWizardKind, plan: GitPlan | null, v: WizardValues): string {
  const action = plan?.action;
  switch (kind) {
    case "create-branch":
      return v.switchTo ? "Create and switch" : "Create branch";
    case "switch":
      return "Switch";
    case "rename-branch":
      return "Rename";
    case "delete-branch":
      return "Delete branch";
    case "merge":
      return plan?.merge?.fast_forward ? "Move forward" : "Merge";
    case "pull":
      return "Pull";
    case "push":
      return "Push";
    case "publish":
      return "Publish";
    case "fetch":
      return "Check now";
    case "add-remote":
      return "Add remote";
    case "remove-remote":
      return "Remove";
    case "rename-remote":
      return "Rename";
    case "set-remote-url":
      return "Change address";
    case "stash":
      return "Park changes";
    case "unstash":
      return "Bring back";
    case "drop-stash":
      return "Delete";
    case "revert":
      return "Undo commit";
    case "cherry-pick":
      return "Copy commit";
    case "reset":
      return v.mode === "discard" ? "Move and discard" : "Move branch";
    case "discard":
      return `Discard ${plan?.files?.length ?? v.paths.length} ${(plan?.files?.length ?? v.paths.length) === 1 ? "change" : "changes"}`;
    case "create-tag":
      return "Create tag";
    case "delete-tag":
      return "Delete tag";
    case "push-tag":
      return "Publish tag";
    case "amend":
      return "Change last commit";
    case "abort":
      return "Stop and go back";
    case "restore-files":
      return `Restore ${plan?.files?.length ?? v.paths.length} ${(plan?.files?.length ?? v.paths.length) === 1 ? "file" : "files"}`;
    case "restore-branch":
      return "Bring back";
    case "rewrite":
      return "Rewrite branch";
    case "diverged":
      return action?.action === "reset" ? "Take theirs" : "Merge both";
    case "detached":
      return action?.action === "switch" ? `Back to ${v.target}` : "Create branch";
    default:
      return "Continue";
  }
}

/** The second question of a destructive plan. */
export function destructiveQuestion(kind: GitWizardKind, plan: GitPlan): string {
  const files = plan.files?.length ?? 0;
  switch (plan.action.action) {
    case "discard":
      return `Really throw away the changes to ${files} ${files === 1 ? "file" : "files"}? Undo can bring them back.`;
    case "reset":
      return "Really move the branch and throw away uncommitted changes? Undo can bring them back.";
    case "delete-branch":
      return `Really delete ${plan.action.name}? ${plan.warnings?.[0] ?? ""}`.trim();
    case "drop-stash":
      return "Really delete these parked changes? Undo can bring them back.";
    case "abort":
      return "Really stop and take the changes out of your files? Undo can bring them back as plain changes.";
    case "restore-files":
      return `Really replace ${files} ${files === 1 ? "file" : "files"}? The backup keeps what is there now, so Undo can bring it back.`;
    default:
      return kind === "diverged" ? "Really replace your commits with the server's? Undo can bring them back." : "Really do this?";
  }
}

/** The way out the wizard offers for a block reason. */
export type WayOut = "save" | "park" | "publish" | "diverged" | "force" | "resolve" | "none";

export function wayOut(reason: GitBlockReason): WayOut {
  switch (reason) {
    case "unsaved-changes":
      return "save";
    case "local-changes":
      return "park";
    case "no-upstream":
      return "publish";
    case "diverged":
    case "rejected":
      return "diverged";
    case "not-merged":
      return "force";
    case "in-progress":
      return "resolve";
    default:
      return "none";
  }
}

/** Short form of a ref move for the plan: "main: a1b2c3d → e4f5g6h". */
export function refMove(move: { name: string; before: string | null; after: string | null }): { name: string; before: string; after: string } {
  const name = move.name === "HEAD" ? "You are here" : move.name.replace(/^refs\/(heads|tags|remotes)\//, "");
  const show = (oid: string | null) => (oid === null ? "(none)" : oid === "" ? "new commit" : oid.slice(0, 7));
  return { name, before: show(move.before), after: show(move.after) };
}

/** The Git word a wizard explains at the top. */
export function wizardTerm(kind: GitWizardKind): GitTerm | null {
  const map: Partial<Record<GitWizardKind, GitTerm>> = {
    "create-branch": "branch",
    switch: "current-branch",
    "rename-branch": "branch",
    "delete-branch": "branch",
    merge: "merge",
    pull: "pull",
    push: "push",
    publish: "publish",
    fetch: "fetch",
    "add-remote": "remote",
    "remove-remote": "remote",
    "rename-remote": "remote",
    "set-remote-url": "remote",
    remotes: "remote",
    stash: "stash",
    unstash: "stash",
    "drop-stash": "stash",
    revert: "revert",
    "cherry-pick": "cherry-pick",
    reset: "reset",
    discard: "backup",
    "create-tag": "tag",
    "delete-tag": "tag",
    "push-tag": "tag",
    diverged: "diverged",
    detached: "detached",
    clone: "clone",
    undo: "backup",
    amend: "amend",
    abort: "abort",
    "restore-files": "restore",
    "restore-branch": "restore",
    rewrite: "rewrite",
  };
  return map[kind] ?? null;
}

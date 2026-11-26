// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitPlan, GitRef } from "../types";
import { GLOSSARY } from "../glossary";
import { confirmLabel, destructiveQuestion, initialValues, nameProblem, refMove, toAction, wayOut, wizardTerm, type WizardContext } from "./model";
import type { GitWizardKind } from "./open";
import { refNameProblem, suggestRefName } from "./refname";

const ref = (name: string, kind: GitRef["kind"], current = false): GitRef => ({
  full: kind === "local" ? `refs/heads/${name}` : kind === "tag" ? `refs/tags/${name}` : `refs/remotes/${name}`,
  name,
  kind,
  target: "a".repeat(40),
  current,
  ahead: 0,
  behind: 0,
});
const ctx: WizardContext = {
  current: "feature",
  upstream: "origin/feature",
  refs: [ref("main", "local"), ref("feature", "local", true), ref("origin/main", "remote"), ref("v1.0", "tag")],
};
const plan = (over: Partial<GitPlan>): GitPlan =>
  ({ action: { action: "pull" }, steps: [], refs: [], backup: false, undoable: true, destructive: false, network: false, ...over }) as GitPlan;

describe("wizard values and actions", () => {
  it("presets the obvious choice for each wizard", () => {
    expect(initialValues("merge", {}, ctx).from).toBe("main");
    expect(initialValues("create-branch", {}, ctx).target).toBe("HEAD");
    expect(initialValues("create-branch", { start: "abc1234" }, ctx).target).toBe("abc1234");
    expect(initialValues("create-branch", {}, ctx).switchTo).toBe(true);
    expect(initialValues("rename-branch", {}, ctx).from).toBe("feature");
    expect(initialValues("delete-branch", {}, ctx).name).toBe("main");
    expect(initialValues("publish", {}, ctx).name).toBe("feature");
    expect(initialValues("reset", {}, ctx).target).toBe("HEAD~1");
    expect(initialValues("diverged", {}, ctx).choice).toBe("merge");
    expect(initialValues("diverged", { choice: "theirs" }, ctx).choice).toBe("theirs");
    expect(initialValues("detached", {}, ctx)).toMatchObject({ choice: "branch", target: "main" });
    // The graph's context menus (strand F) use these presets.
    expect(initialValues("publish", { branch: "main" }, ctx).name).toBe("main");
    expect(initialValues("delete-branch", { branch: "main" }, ctx).name).toBe("main");
    expect(initialValues("reset", { target: "abc1234", mode: "discard" }, ctx)).toMatchObject({ target: "abc1234", mode: "discard" });
    expect(initialValues("switch", { target: "main" }, ctx).target).toBe("main");
    expect(initialValues("cherry-pick", { commit: "abc1234" }, ctx).commit).toBe("abc1234");
    expect(initialValues("create-tag", { target: "abc1234" }, ctx).target).toBe("abc1234");
  });

  it("builds the action only when everything required is there", () => {
    const v = (kind: GitWizardKind, preset: Record<string, unknown> = {}) => initialValues(kind, preset, ctx);
    expect(toAction("create-branch", v("create-branch"), ctx)).toBeNull();
    expect(toAction("create-branch", { ...v("create-branch"), name: " docs " }, ctx)).toEqual({ action: "create-branch", name: "docs", start: "HEAD", switch: true });
    expect(toAction("merge", v("merge"), ctx)).toEqual({ action: "merge", from: "main" });
    expect(toAction("add-remote", { ...v("add-remote"), name: "origin", url: "" }, ctx)).toBeNull();
    expect(toAction("stash", v("stash"), ctx)).toEqual({ action: "stash" });
    expect(toAction("revert", v("revert"), ctx)).toBeNull();
    expect(toAction("reset", { ...v("reset"), mode: "discard" }, ctx)).toEqual({ action: "reset", target: "HEAD~1", mode: "discard" });
    expect(toAction("create-tag", { ...v("create-tag"), name: "v2" }, ctx)).toEqual({ action: "create-tag", name: "v2", target: "HEAD", message: undefined });
  });

  it("turns the guided choices into plain actions", () => {
    expect(toAction("diverged", initialValues("diverged", {}, ctx), ctx)).toEqual({ action: "pull" });
    expect(toAction("diverged", initialValues("diverged", { choice: "theirs" }, ctx), ctx)).toEqual({ action: "reset", target: "origin/feature", mode: "discard" });
    expect(toAction("detached", { ...initialValues("detached", {}, ctx), name: "rescue" }, ctx)).toEqual({ action: "create-branch", name: "rescue", start: "HEAD", switch: true });
    expect(toAction("detached", initialValues("detached", { choice: "back" }, ctx), ctx)).toEqual({ action: "switch", target: "main" });
  });

  it("checks names while typing, like git check-ref-format", () => {
    const v = initialValues("create-branch", {}, ctx);
    expect(nameProblem("create-branch", { ...v, name: "" })).toBeNull();
    expect(nameProblem("create-branch", { ...v, name: "feature/ok" })).toBeNull();
    for (const bad of ["a..b", "-x", "x/", "x.lock", "a b", "a~b", "HEAD", ".hidden", "a//b", "a@{1}"]) expect(refNameProblem(bad), bad).toMatch(/cannot be used/);
    expect(nameProblem("merge", { ...v, name: "a..b" })).toBeNull();
    expect(suggestRefName("Fix the  login bug!")).toBe("fix-the-login-bug");
    expect(suggestRefName("  /feature/New Thing. ")).toBe("feature/new-thing");
  });
});

describe("wizard words", () => {
  it("labels the confirm button with what happens", () => {
    const v = initialValues("create-branch", {}, ctx);
    expect(confirmLabel("create-branch", null, v)).toBe("Create and switch");
    expect(confirmLabel("create-branch", null, { ...v, switchTo: false })).toBe("Create branch");
    expect(confirmLabel("merge", plan({ merge: { fast_forward: true, commits: [], files: [] } as unknown as GitPlan["merge"] }), v)).toBe("Move forward");
    expect(confirmLabel("discard", plan({ files: ["a", "b"] }), v)).toBe("Discard 2 changes");
    expect(confirmLabel("diverged", plan({ action: { action: "reset", target: "x", mode: "discard" } }), v)).toBe("Take theirs");
  });

  it("asks a destructive question that names the way back", () => {
    expect(destructiveQuestion("discard", plan({ action: { action: "discard", paths: [] }, files: ["a"] }))).toMatch(/1 file\? Undo can bring them back/);
    expect(destructiveQuestion("diverged", plan({ action: { action: "reset", target: "x", mode: "discard" } }))).toMatch(/Undo/);
  });

  it("offers a way out for every block it can solve", () => {
    expect(wayOut("unsaved-changes")).toBe("save");
    expect(wayOut("local-changes")).toBe("park");
    expect(wayOut("no-upstream")).toBe("publish");
    expect(wayOut("rejected")).toBe("diverged");
    expect(wayOut("not-merged")).toBe("force");
    expect(wayOut("in-progress")).toBe("resolve");
  });

  it("shows ref moves in short words", () => {
    expect(refMove({ name: "refs/heads/main", before: "a".repeat(40), after: "" })).toEqual({ name: "main", before: "aaaaaaa", after: "new commit" });
    expect(refMove({ name: "HEAD", before: null, after: "b".repeat(40) })).toEqual({ name: "You are here", before: "(none)", after: "bbbbbbb" });
  });

  it("explains a Git word in every wizard", () => {
    const kinds: GitWizardKind[] = ["create-branch", "merge", "pull", "push", "publish", "stash", "revert", "cherry-pick", "reset", "create-tag", "diverged", "detached", "clone", "undo", "remotes"];
    for (const k of kinds) {
      const term = wizardTerm(k);
      expect(term, k).not.toBeNull();
      expect(GLOSSARY[term!].text.length, k).toBeGreaterThan(20);
    }
  });
});

describe("wave 15 wizards (strand D)", () => {
  const values = (kind: GitWizardKind, preset: Record<string, unknown>) => initialValues(kind, preset, ctx);

  it("builds the new actions from the presets other views pass", () => {
    expect(toAction("amend", values("amend", { message: "better" }), ctx)).toEqual({ action: "amend", message: "better" });
    expect(toAction("amend", values("amend", { paths: ["/a.yml"] }), ctx)).toEqual({ action: "amend", paths: ["/a.yml"] });
    expect(toAction("amend", values("amend", {}), ctx)).toBeNull();
    expect(toAction("abort", values("abort", {}), ctx)).toEqual({ action: "abort" });
    expect(toAction("restore-files", values("restore-files", { rev: "abc", paths: ["/a.yml"] }), ctx)).toEqual({ action: "restore-files", rev: "abc", paths: ["/a.yml"] });
    expect(toAction("restore-files", values("restore-files", { paths: ["/a.yml"] }), ctx)).toMatchObject({ rev: "HEAD" });
    expect(toAction("restore-files", values("restore-files", {}), ctx)).toBeNull();
    expect(toAction("restore-branch", values("restore-branch", { name: "old", target: "abc" }), ctx)).toEqual({ action: "restore-branch", name: "old", target: "abc" });
    expect(toAction("rewrite", values("rewrite", { branch: "feature", tip: "abc", summary: "Dropped c2" }), ctx)).toEqual({ action: "rewrite", branch: "feature", tip: "abc", summary: "Dropped c2" });
    expect(toAction("stash", values("stash", { paths: ["/a.yml"] }), ctx)).toEqual({ action: "stash", paths: ["/a.yml"] });
    expect(toAction("stash", values("stash", {}), ctx)).toEqual({ action: "stash" });
  });

  it("says what each does, asks before replacing files, and explains its word", () => {
    expect(confirmLabel("restore-files", plan({ files: ["/a", "/b"] }), values("restore-files", {}))).toBe("Restore 2 files");
    expect(destructiveQuestion("abort", plan({ action: { action: "abort" }, destructive: true }))).toContain("Undo");
    expect(destructiveQuestion("restore-files", plan({ action: { action: "restore-files", rev: "HEAD", paths: [] }, files: ["/a"] }))).toContain("1 file");
    expect(nameProblem("restore-branch", values("restore-branch", { name: "bad name" }))).not.toBeNull();
    for (const kind of ["amend", "abort", "restore-files", "restore-branch", "rewrite"] as GitWizardKind[]) {
      const term = wizardTerm(kind);
      expect(term).not.toBeNull();
      expect(GLOSSARY[term!].text.length).toBeGreaterThan(10);
    }
    expect(wayOut("published")).toBe("none");
  });
});

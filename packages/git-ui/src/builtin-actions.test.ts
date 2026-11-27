// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { withBuiltinFileActions } from "./builtin-actions";
import { extensionGitActions, gitActionEntries } from "./extension-git";
import type { GitContent } from "./host";
import { GitViews } from "./views.svelte";
import type { GitExtension, GitActionTarget, ProvidedGitExtensions } from "./extensions";

const FILE: GitActionTarget = { kind: "file", repository: "/r", path: "/src/a.ts" };
const provided = (extensions: GitExtension[]): ProvidedGitExtensions => ({ extensions, ctx: {} });
const keys = (p: ProvidedGitExtensions | null, t: GitActionTarget) => extensionGitActions(p, t).map((r) => r.key);

describe("the kit's Blame and File history actions", () => {
  it("are offered on file targets without content or extensions", () => {
    const views = new GitViews();
    const p = withBuiltinFileActions(null, {}, views);
    expect(keys(p, FILE)).toEqual(["git/blame", "git/file-history"]);
    const entries = gitActionEntries(p, FILE, () => {});
    expect(entries.map((e) => ("label" in e ? e.label : "|"))).toEqual(["|", "Blame", "File history"]);
    (entries[1] as { run: () => void }).run();
    expect(views.current).toEqual({ kind: "blame", path: "/src/a.ts", rev: undefined });
    (entries[2] as { run: () => void }).run();
    expect(views.current).toEqual({ kind: "file-history", path: "/src/a.ts" });
  });

  it("blame the file at the revision of the target", () => {
    const views = new GitViews();
    const p = withBuiltinFileActions(null, {}, views);
    extensionGitActions(p, { ...FILE, rev: "abc" })[0]!.action.run();
    expect(views.current).toEqual({ kind: "blame", path: "/src/a.ts", rev: "abc" });
  });

  it("are not offered on folders or on other targets", () => {
    const p = withBuiltinFileActions(null, {}, new GitViews());
    expect(keys(p, { ...FILE, directory: true })).toEqual([]);
    expect(keys(p, { kind: "commit", repository: "/r", commit: "a" })).toEqual([]);
    expect(keys(p, { kind: "repository", repository: "/r" })).toEqual([]);
    expect(keys(p, { kind: "conflict", repository: "/r", path: "/a" })).toEqual([]);
  });

  it("stay behind the product's own actions", () => {
    const x: GitExtension = { id: "p", gitActions: (_c, t) => (t.kind === "file" ? [{ id: "open", label: "Open", run: () => {} }] : []) };
    expect(keys(withBuiltinFileActions(provided([x]), {}, new GitViews()), FILE)).toEqual(["p/open", "git/blame", "git/file-history"]);
  });

  it("give way to an extension action with the same id", () => {
    const x: GitExtension = { id: "gt", gitActions: (_c, t) => (t.kind === "file" ? [{ id: "blame", label: "Blame", run: () => {} }] : []) };
    expect(keys(withBuiltinFileActions(provided([x]), {}, new GitViews()), FILE)).toEqual(["gt/blame", "git/file-history"]);
    const y: GitExtension = { id: "hist", gitActions: () => [{ id: "file-history", label: "History", run: () => {} }] };
    expect(keys(withBuiltinFileActions(provided([x, y]), {}, new GitViews()), FILE)).toEqual(["gt/blame", "hist/file-history"]);
  });

  it("ignore a failing extension", () => {
    const bad: GitExtension = {
      id: "bad",
      gitActions: () => {
        throw new Error("no");
      },
    };
    const p = withBuiltinFileActions(provided([bad]), {}, new GitViews());
    expect(keys(p, FILE)).toEqual(["git/blame", "git/file-history"]);
  });

  it("are left out when the host has content, or there are no views", () => {
    const content = {} as GitContent;
    const base = provided([]);
    expect(withBuiltinFileActions(base, { content }, new GitViews())).toBe(base);
    expect(withBuiltinFileActions(null, { content }, new GitViews())).toBeNull();
    expect(withBuiltinFileActions(base, {}, undefined)).toBe(base);
  });
});

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { commandsFor, withBuiltinFileActions } from "./builtin-actions";
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

describe("commandsFor", () => {
  it("lists every action of every extension on the target, in order, with its group", () => {
    const a: GitExtension = { id: "a", gitActions: () => [{ id: "one", label: "One", run: () => {} }, { id: "ai", label: "Ask", group: "ai", badge: "beta", run: () => {} }] };
    const b: GitExtension = { id: "b", gitActions: (_c, t) => (t.kind === "repository" ? [{ id: "two", label: "Two", icon: "x", run: () => {} }] : []) };
    const repo: GitActionTarget = { kind: "repository", repository: "/r" };
    const out = commandsFor(provided([a, b]), repo);
    expect(out.map((c) => [c.key, c.label, c.group ?? null, c.badge ?? null])).toEqual([
      ["a/one", "One", null, null],
      ["a/ai", "Ask", "ai", "beta"],
      ["b/two", "Two", null, null],
    ]);
    expect(commandsFor(provided([a, b]), { kind: "commit", repository: "/r", commit: "c" }).map((c) => c.key)).toEqual(["a/one", "a/ai"]);
    expect(commandsFor(null, repo)).toEqual([]);
  });

  it("runs an action and reports a failure instead of throwing", async () => {
    const seen: string[] = [];
    const x: GitExtension = {
      id: "x",
      gitActions: () => [
        { id: "ok", label: "Ok", run: () => void seen.push("ran") },
        { id: "sync", label: "Sync", run: () => { throw new Error("no sync"); } },
        { id: "async", label: "Async", run: async () => { throw new Error("no async"); } },
      ],
    };
    const errors: string[] = [];
    const [ok, sync, asy] = commandsFor(provided([x]), { kind: "repository", repository: "/r" }, (m) => errors.push(m));
    ok!.run();
    sync!.run();
    asy!.run();
    await new Promise((r) => setTimeout(r, 0));
    expect(seen).toEqual(["ran"]);
    expect(errors).toEqual(["no sync", "no async"]);
  });
});

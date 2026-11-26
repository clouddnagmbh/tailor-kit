// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it, vi } from "vitest";

import type { ExtensionGitViewProps, ExtensionStagingProps, GitExtension as AppExtension, ProvidedGitExtensions as ProvidedExtensions } from "./extensions";
import {
  dismissalKey,
  extensionCommitFormActions,
  extensionFileEditors,
  extensionGitActions,
  extensionGitHints,
  extensionGitView,
  extensionGitViews,
  extensionStaging,
  gitActionEntries,
  pickHint,
  withRefActions,
} from "./extension-git";

// The package never reads the product's context: any value stands in for it.
function provided(extensions: AppExtension[]): ProvidedExtensions {
  return { extensions, ctx: {} };
}

// Components are never rendered here; any function stands in for one.
const View = (() => {}) as unknown as import("svelte").Component<ExtensionGitViewProps>;
const Staging = (() => {}) as unknown as import("svelte").Component<ExtensionStagingProps>;

describe("Git contributions of extensions (wave 15 interface)", () => {
  const one: AppExtension = {
    id: "one",
    gitActions: (_ctx, target) =>
      target.kind === "commits" ? [{ id: "squash", label: "Combine commits", run: () => {} }] : target.kind === "file" ? [{ id: "open", label: "Open", inline: true, run: () => {} }] : [],
    gitViews: () => [{ id: "editor", title: "Edit history", component: View }],
    fileEditors: () => [
      { id: "merge", title: "Merge editor", kind: "merge", accepts: (p) => p.endsWith(".ts"), component: View as never },
      { id: "diff", title: "Diff", kind: "diff", component: View as never },
    ],
    commitFormActions: (_ctx, target) => (target.staged ? [{ id: "template", label: "Template", run: () => {} }] : []),
    staging: { title: "Staged", component: Staging, active: (_ctx, repo) => repo === "/work/api" },
    gitHints: (_ctx, target) => (target.view === "changes" ? [{ id: "tip", text: "A tip.", scope: "target" }] : []),
  };
  const broken: AppExtension = {
    id: "broken",
    gitActions: () => {
      throw new Error("no");
    },
    gitViews: () => {
      throw new Error("no");
    },
    staging: {
      title: "Broken",
      component: Staging,
      active: () => {
        throw new Error("no");
      },
    },
  };
  const second: AppExtension = { id: "second", staging: { title: "Always", component: Staging } };

  it("collects actions per target, keyed by extension, and skips a failing extension", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const p = provided([broken, one]);
    expect(extensionGitActions(p, { kind: "commits", repository: "/r", commits: ["a", "b"] }).map((r) => r.key)).toEqual(["one/squash"]);
    expect(extensionGitActions(p, { kind: "file", repository: "/r", path: "/a.ts" })[0]?.action.inline).toBe(true);
    expect(extensionGitActions(p, { kind: "repository", repository: "/r" })).toEqual([]);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it("finds views by key and editors by kind and path", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const p = provided([broken, one]);
    expect(extensionGitViews(p).map((r) => r.key)).toEqual(["one/editor"]);
    expect(extensionGitView(p, "one/editor")?.title).toBe("Edit history");
    expect(extensionGitView(p, "one/missing")).toBeNull();
    expect(extensionFileEditors(p, "merge", "/src/a.ts").map((r) => r.key)).toEqual(["one/merge"]);
    expect(extensionFileEditors(p, "merge", "/logo.png")).toEqual([]);
    expect(extensionFileEditors(p, "diff", "/logo.png").map((r) => r.key)).toEqual(["one/diff"]);
    warn.mockRestore();
  });

  it("puts the first active staging view in charge, else none", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(extensionStaging(provided([broken, one, second]), "/work/api")?.extension).toBe("one");
    expect(extensionStaging(provided([broken, one, second]), "/other")?.extension).toBe("second");
    expect(extensionStaging(provided([one]), "/other")).toBeNull();
    expect(extensionStaging(null, "/work/api")).toBeNull();
    warn.mockRestore();
  });

  it("collects hints per Git view, dismissed per repository with scope target", () => {
    const changes = { place: "git", repository: "/work/api", view: "changes" } as const;
    const refs = extensionGitHints(provided([one]), changes);
    expect(refs.map((r) => r.key)).toEqual(["one/tip"]);
    expect(dismissalKey(refs[0]!, changes)).toBe("one/tip@git:/work/api");
    expect(pickHint(refs, changes, new Set(["one/tip@git:/work/api"]))).toBeNull();
    expect(extensionGitHints(provided([one]), { ...changes, view: "graph" })).toEqual([]);
  });

  it("offers commit form actions by target", () => {
    const p = provided([one]);
    const message = { get: () => "", set: () => {} };
    expect(extensionCommitFormActions(p, { repository: "/r", message, paths: [], staged: true }).map((r) => r.key)).toEqual(["one/template"]);
    expect(extensionCommitFormActions(p, { repository: "/r", message, paths: [], staged: false })).toEqual([]);
  });

  it("hands hints the situation the view reports", () => {
    const seen: unknown[] = [];
    const watcher: AppExtension = {
      id: "watch",
      gitHints: (_ctx, t) => {
        seen.push([t.view, t.selected, t.dragging, t.hunks]);
        return t.view === "graph" && t.selected === 2 && t.dragging ? [{ id: "range", text: "Two commits." }] : [];
      },
    };
    const p = provided([watcher]);
    const graph = { place: "git", repository: "/r", view: "graph" } as const;
    expect(extensionGitHints(p, { ...graph, selected: 1, dragging: false })).toEqual([]);
    expect(extensionGitHints(p, { ...graph, selected: 2, dragging: true }).map((r) => r.key)).toEqual(["watch/range"]);
    expect(extensionGitHints(p, { place: "git", repository: "/r", view: "changes", hunks: 7 })).toEqual([]);
    expect(seen).toEqual([["graph", 1, false, undefined], ["graph", 2, true, undefined], ["changes", undefined, undefined, 7]]);
  });

  it("turns actions into menu entries behind a separator and reports a failing one", async () => {
    const fail = vi.fn();
    const ran: string[] = [];
    const ext: AppExtension = {
      id: "m",
      gitActions: (_ctx, t) =>
        t.kind === "ref"
          ? [
              { id: "ok", label: "Fine", run: () => void ran.push(t.ref) },
              { id: "sync", label: "Throws", run: () => { throw new Error("sync"); } },
              { id: "async", label: "Rejects", run: () => Promise.reject(new Error("async")) },
            ]
          : [],
    };
    const p = provided([ext]);
    expect(gitActionEntries(p, { kind: "commit", repository: "/r", commit: "a" }, fail)).toEqual([]);
    const entries = gitActionEntries(p, { kind: "ref", repository: "/r", ref: "refs/heads/main" }, fail);
    expect(entries[0]).toEqual({ separator: true });
    expect(entries.slice(1).map((e) => ("label" in e ? e.label : ""))).toEqual(["Fine", "Throws", "Rejects"]);
    for (const e of entries.slice(1)) if ("run" in e) await e.run?.();
    await Promise.resolve();
    expect(ran).toEqual(["refs/heads/main"]);
    expect(fail.mock.calls.map((c) => c[0])).toEqual(["sync", "async"]);
  });

  it("adds ref actions to the submenu of each branch or tag of a commit's menu", () => {
    const ext: AppExtension = {
      id: "r",
      gitActions: (_ctx, t) => (t.kind === "ref" ? [{ id: "x", label: `On ${t.ref}`, run: () => {} }] : []),
    };
    const menu = withRefActions(
      provided([ext]),
      "/r",
      [
        { id: "copy-id", label: "Copy" },
        { id: "ref:refs/heads/main", label: "main", items: [{ id: "switch", label: "Switch" }] },
      ],
      () => {},
    );
    const sub = (menu[1] as { items: { label?: string }[] }).items;
    expect(sub.map((i) => i.label)).toEqual(["Switch", undefined, "On refs/heads/main"]);
    expect(menu[0]).toEqual({ id: "copy-id", label: "Copy" });
    // Without actions the menu is returned as it was.
    expect(withRefActions(provided([]), "/r", menu, () => {})).toEqual(menu);
  });
});

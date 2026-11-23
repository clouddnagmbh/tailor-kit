// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it, vi } from "vitest";

import { MockBackend } from "../commands/mock";
import type { AppExtension, ExtensionGitViewProps, ExtensionStagingProps } from "../extensions";
import { AppState } from "../state/app.svelte";
import { createExtensionContext } from "./extension-context";
import {
  extensionCommitFormActions,
  extensionFileEditors,
  extensionGitActions,
  extensionGitHints,
  extensionGitView,
  extensionGitViews,
  extensionStaging,
} from "./extension-git";
import { dismissalKey, pickHint } from "./extension-hints";
import type { ProvidedExtensions } from "./extension-tabs";

function provided(extensions: AppExtension[]): ProvidedExtensions {
  const backend = new MockBackend();
  return { extensions, ctx: createExtensionContext(backend, new AppState(backend)) };
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
});

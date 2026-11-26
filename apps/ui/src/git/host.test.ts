// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Seam N6: what the Git views do with a host that has no `content` (plain Git), and with one that
// has; RequestTailor's host over `AppState`.

import { describe, expect, it } from "vitest";

import { MockBackend } from "../commands/mock";
import { AppState } from "../state/app.svelte";
import { createGitHost, RT_BRAND } from "../state/git-host";
import { gitWord } from "./glossary";
import { historyCommands } from "./history/commands";
import { fileOwnership, getGitHost, hasRepository, type GitContent, type GitOwner } from "./host";
import { testGitHost, TEST_BRAND } from "./host-testing";
import { reviewCommands } from "../review/commands";
import { GitViews } from "./views.svelte";
import { gitWizardCommands } from "./wizards/commands";
import { openGitWizard } from "./wizards/open";

const OWNERS: GitOwner[] = [
  { id: "a", name: "Alpha", location: "/r/a", repo: "/r" },
  { id: "b", name: "Beta", location: "/r/b", repo: "/r" },
];

const content = (activeOwner: string | null): GitContent =>
  ({ activeOwner, owners: OWNERS, activate: async () => {}, tree: null, activeFile: null }) as unknown as GitContent;

describe("GitHost without content (plain Git)", () => {
  it("shows no field view and offers no collection to open, whatever a file says", () => {
    const host = testGitHost();
    expect(host.content).toBeUndefined();
    expect(fileOwnership(host, "a")).toEqual({ semantic: false, activatable: null, other: undefined });
    expect(fileOwnership(host, null)).toEqual({ semantic: false, activatable: null, other: undefined });
  });

  it("has a repository only when one is active", () => {
    expect(hasRepository(testGitHost())).toBe(false);
    expect(hasRepository(testGitHost({ activeRepository: "/r" }))).toBe(true);
  });

  it("lists no wizard commands without a repository and the generic ones with it", () => {
    const views = new GitViews();
    expect(gitWizardCommands(testGitHost(), views)).toEqual([]);
    const ids = gitWizardCommands(testGitHost({ activeRepository: "/r" }), views).map((c) => c.id);
    expect(ids.length).toBeGreaterThan(3);
  });

  it("opens the wizard dialog through the host", async () => {
    const host = testGitHost({ activeRepository: "/r" });
    await openGitWizard(host, { kind: "stash" });
    expect(host.calls.dialogs.map((d) => d.title)).toEqual(["Park changes"]);
  });

  it("answers 'Blame this request' with a notice instead of opening a view", () => {
    const host = testGitHost({ activeRepository: "/r" });
    const blame = historyCommands(host, new GitViews()).find((c) => c.id === "git-history-blame")!;
    void blame.run();
    expect(host.calls.errors).toEqual(["Open a request first."]);
  });

  it("creates a pull request through the host's dialog", () => {
    const host = testGitHost({ activeRepository: "/r" });
    const create = reviewCommands(host, new GitViews()).find((c) => c.id === "git:review-create")!;
    void create.run();
    expect(host.calls.dialogs).toHaveLength(1);
  });
});

describe("GitHost with content", () => {
  it("knows a file of the active owner, and one of another owner it can activate", () => {
    const host = testGitHost({ content: content("a") });
    expect(fileOwnership(host, "a")).toEqual({ semantic: true, activatable: null, other: undefined });
    expect(fileOwnership(host, "b")).toEqual({ semantic: false, activatable: "b", other: OWNERS[1] });
    expect(fileOwnership(host, "gone")).toEqual({ semantic: false, activatable: "gone", other: undefined });
  });

  it("has a repository through the active owner alone", () => {
    expect(hasRepository(testGitHost({ content: content("a") }))).toBe(true);
    expect(hasRepository(testGitHost({ content: content(null) }))).toBe(false);
  });

  it("opens the active file in blame and history", () => {
    const base = content("a");
    const host = testGitHost({ activeRepository: "/r", content: { ...base, activeFile: "health.yml" } });
    const views = new GitViews();
    const command = (id: string) => historyCommands(host, views).find((c) => c.id === id)!;
    void command("git-history-blame").run();
    expect(views.current).toEqual({ kind: "blame", path: "health.yml" });
  });
});

describe("brand", () => {
  it("names the Git word for the backup refs after the product", () => {
    expect(gitWord("backup", TEST_BRAND)).toBe("refs/testtailor/backup");
    expect(gitWord("backup", RT_BRAND)).toBe("refs/requesttailor/backup");
    expect(gitWord("fetch", RT_BRAND)).toBe("fetch");
  });

  it("keeps RequestTailor's literals", () => {
    expect(RT_BRAND).toEqual({ productName: "RequestTailor", commitMime: "application/x-requesttailor-commit", refNamespace: "requesttailor" });
  });
});

describe("getGitHost", () => {
  it("refuses outside a host", () => {
    expect(() => getGitHost()).toThrow();
  });
});

describe("RequestTailor's host over AppState", () => {
  it("reads and acts on the app", async () => {
    const app = new AppState(new MockBackend());
    const host = createGitHost(app);
    expect(host.brand).toBe(RT_BRAND);
    expect(host.backend).toBe(app.backend);
    expect(host.content?.activeOwner).toBeNull();
    expect(host.activeRepository).toBeNull();
    expect(host.dialogOpen).toBe(false);
    host.notifyError("boom");
    expect(app.error).toBe("boom");
    host.show({ kind: "graph" });
    expect(app.view?.view).toEqual({ kind: "git", view: { kind: "graph" } });
    const closed = host.showDialog({ title: "T", component: (() => {}) as never });
    expect(host.dialogOpen).toBe(true);
    app.closeDialog(7);
    expect(await closed).toBe(7);
    app.prefillCommitMessage("msg");
    expect(host.commitPrefill?.text).toBe("msg");
    expect(host.takeCommitPrefill()).toBe("msg");
    expect(host.commitPrefill).toBeNull();
  });

  it("follows the open collection", async () => {
    const app = new AppState(new MockBackend());
    await app.open();
    const host = createGitHost(app);
    expect(host.content?.activeOwner).toBe(app.activeCollection);
    expect(host.cacheScope).toBe(app.activeCollection);
    expect(host.reloadCount).toBe(app.loadCount);
    expect(host.unsaved()).toEqual(app.dirty);
    expect(host.content?.owners.map((o) => o.id)).toContain(app.activeCollection);
    expect(host.content?.tree).toBe(app.tree);
  });
});

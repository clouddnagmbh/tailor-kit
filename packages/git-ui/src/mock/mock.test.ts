// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { createMockGitBackend, MOCK_REMOTE_HOST, MOCK_REVIEW_TOKEN, mockGit, type MockGit } from "./index";

const who = { name: "Ada", email: "ada@example.com" };

async function repo(scenario?: Parameters<MockGit["scenario"]>[0]) {
  const git = mockGit();
  await git.setIdentity(who);
  if (scenario) git.scenario(scenario);
  return git;
}

describe("the Git mock over plain text files", () => {
  it("starts clean and in sync with origin", async () => {
    const git = await repo();
    expect(await git.status()).toEqual({ repo: true, branch: "main", upstream: "origin/main", ahead: 0, behind: 0, files: [], merging: false });
    expect((await git.log(5)).map((c) => c.summary)).toEqual(["Initial commit"]);
    expect(Object.keys((await git.treeAt("HEAD")).files).sort()).toEqual(["README.md", "docs/blocked.txt", "docs/guide.md", "docs/old.txt", "src/app.txt", "src/lib.txt"]);
  });

  it("names its scope in messages", async () => {
    const git = await repo("none");
    await expect(git.log(1).then(() => git.branches())).resolves.toEqual([]);
    await expect(git.diff("README.md")).rejects.toThrow("The repository is not in a Git repository.");
  });

  it("the scenarios of the Git panel have the shapes the panel expects", async () => {
    const shape = async (name: Parameters<MockGit["scenario"]>[0]) => {
      const s = await (await repo(name)).status();
      return { ahead: s.ahead, behind: s.behind, files: s.files.length, branch: s.branch, upstream: s.upstream };
    };
    expect(await shape("ahead")).toMatchObject({ ahead: 1, behind: 0, files: 0 });
    expect(await shape("behind")).toMatchObject({ ahead: 0, behind: 2 });
    expect(await shape("diverged")).toMatchObject({ ahead: 1, behind: 1 });
    expect(await shape("unpublished")).toMatchObject({ branch: "feature/login", upstream: null });
    expect(await shape("detached")).toMatchObject({ branch: null });
    expect(await shape("showcase")).toMatchObject({ files: 3, ahead: 2 });
    expect(await shape("no-remote")).toMatchObject({ upstream: null });
    expect(await shape("empty")).toMatchObject({ branch: null });
  });

  it("a conflicting pull stops, shows the three sides and finishes after a resolution", async () => {
    const git = await repo("conflict");
    expect(await git.pull()).toEqual({ outcome: "conflicts", paths: ["README.md"] });
    const c = await git.conflict("README.md");
    expect(c.base).toBe("# Demo\n\nA small project.\n");
    expect(c.ours).toContain("Local line.");
    expect(c.theirs).toContain("Remote line.");
    await git.resolve("README.md", "theirs");
    const done = await git.commit("Merge", ["README.md"]);
    expect(done.summary).toBe("Merge");
    expect((await git.status()).merging).toBe(false);
  });

  it("a wizard plans, applies and undoes", async () => {
    const git = await repo();
    const action = { action: "create-branch" as const, name: "feature/x", start: "HEAD", switch: true };
    const plan = await git.plan(action);
    expect(plan.blocked).toBeFalsy();
    await git.apply(action);
    expect((await git.status()).branch).toBe("feature/x");
    const last = await git.lastAction();
    expect(last).not.toBeNull();
    await git.undo(last!.id);
    expect((await git.status()).branch).toBe("main");
  });

  it("the working copy and the commit graph carry no product fields", async () => {
    const git = await repo("showcase");
    const copy = await git.workingCopy();
    expect(copy.files.map((f) => f.path)).toEqual(["/README.md", "/docs/blocked.txt", "/src/new.txt"]);
    expect(copy.files.every((f) => Object.keys(f).every((k) => ["path", "state", "unstaged"].includes(k)))).toBe(true);
    const page = await git.graph({ limit: 50 });
    expect(page.commits.length).toBeGreaterThan(3);
    const changes = await git.changes(null, "HEAD");
    expect(changes.every((f) => Object.keys(f).every((k) => ["path", "change", "binary", "from"].includes(k)))).toBe(true);
  });
});

describe("the whole mock backend", () => {
  it("lists repositories, initialises one and clones", async () => {
    const b = createMockGitBackend();
    expect(b.kind).toBe("mock");
    expect(b.repositories.list().map((r) => r.id)).toEqual(["mock://demo"]);
    expect((await b.repositories.init())?.id).toBe("mock://new-repo");
    expect(await b.repositories.summary("mock://new-repo")).toMatchObject({ branch: null, changes: 0 });
    expect((await b.repositories.summary("mock://demo")).branch).toBe("main");
    await expect(b.repositories.summary("mock://nope")).rejects.toMatchObject({ kind: "not-found" });
  });

  it("reviews a pull request: host, token, list, open", async () => {
    const b = createMockGitBackend();
    expect(await b.review.repo()).toBeNull();
    await b.review.setHosts([{ host: MOCK_REMOTE_HOST, kind: "github", api: "https://git.example.com/api/v3" }]);
    await expect(b.review.list()).rejects.toMatchObject({ kind: "no-token" });
    await b.review.setToken(MOCK_REMOTE_HOST, MOCK_REVIEW_TOKEN);
    expect((await b.review.list()).map((p) => p.number)).toEqual([1, 2]);
    const session = await b.review.open(1);
    expect(session.head_ref).not.toBe(session.merge_base);
    const head = await b.git.treeAt(session.head_ref);
    expect(head.files["docs/added.md"]).toBe("# Added\n");
    expect((await b.review.ci(1)).state).toBe("failure");
  });
});

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitGraphCommit, GitRef } from "../types";
import { commitMenu, matches, semanticFor, refLabels, refMenu, relativeTime, selectTarget, syncText, type GraphMenuEntry, type GraphMenuItem } from "./model";

const ref = (full: string, kind: GitRef["kind"], target: string, extra: Partial<GitRef> = {}): GitRef => ({
  full,
  name: full.replace(/^refs\/(heads|remotes|tags)\//, ""),
  kind,
  target,
  current: false,
  ahead: 0,
  behind: 0,
  ...extra,
});

const main = ref("refs/heads/main", "local", "c1", { current: true, upstream: "origin/main", ahead: 2, behind: 1 });
const feature = ref("refs/heads/feature", "local", "c1");
const originMain = ref("refs/remotes/origin/main", "remote", "c1");
const originOther = ref("refs/remotes/origin/other", "remote", "c1");
const v1 = ref("refs/tags/v1", "tag", "c1", { message: "First" });
const byFull = new Map([main, feature, originMain, originOther, v1].map((r) => [r.full, r]));

const commit = (oid: string, extra: Partial<GitGraphCommit> = {}): GitGraphCommit => ({
  oid,
  summary: "Add pagination to orders",
  author: "Grace Hopper",
  email: "grace@example.com",
  time: 0,
  parents: [],
  ...extra,
});

const items = (entries: GraphMenuEntry[]) => entries.filter((e): e is GraphMenuItem => "id" in e);
const byId = (entries: GraphMenuEntry[], id: string) => items(entries).find((e) => e.id === id)!;

describe("ref labels", () => {
  it("fold a remote branch into its local branch when both point at the commit", () => {
    const labels = refLabels([v1.full, originOther.full, feature.full, originMain.full, main.full], byFull);
    expect(labels.map((l) => [l.ref.name, l.tracking?.name])).toEqual([
      ["main", "origin/main"],
      ["feature", undefined],
      ["origin/other", undefined],
      ["v1", undefined],
    ]);
  });

  it("keep the remote branch apart when it is elsewhere", () => {
    const away = new Map(byFull);
    away.set(originMain.full, { ...originMain, target: "c0" });
    expect(refLabels([main.full, originMain.full], away).map((l) => l.tracking)).toEqual([undefined, undefined]);
  });

  it("put a detached HEAD first and skip unknown refs", () => {
    const head = ref("HEAD", "head", "c1", { current: true, name: "HEAD" });
    const map = new Map([...byFull, [head.full, head]]);
    expect(refLabels([v1.full, "refs/heads/gone", head.full], map).map((l) => l.ref.name)).toEqual(["HEAD", "v1"]);
    expect(refLabels(undefined, map)).toEqual([]);
  });
});

describe("dates", () => {
  const now = Date.UTC(2026, 10, 23, 12, 0, 0);
  const at = (secondsAgo: number) => Math.floor(now / 1000) - secondsAgo;
  it("read relative for the last week, then as a date", () => {
    expect(relativeTime(at(10), now)).toBe("just now");
    expect(relativeTime(at(60), now)).toBe("1 minute ago");
    expect(relativeTime(at(5 * 60), now)).toBe("5 minutes ago");
    expect(relativeTime(at(3 * 3600), now)).toBe("3 hours ago");
    expect(relativeTime(at(30 * 3600), now)).toBe("yesterday");
    expect(relativeTime(at(4 * 86400), now)).toBe("4 days ago");
    expect(relativeTime(at(30 * 86400), now)).toMatch(/^Oct 2[34]$/);
    expect(relativeTime(at(400 * 86400), now)).toMatch(/2025$/);
  });
});

describe("search", () => {
  it("finds words of the message, the author, an id prefix and ref names", () => {
    const c = commit("abcdef1234", { refs: [feature.full] });
    expect(matches(c, byFull, "pagination")).toBe(true);
    expect(matches(c, byFull, "GRACE")).toBe(true);
    expect(matches(c, byFull, "abcd")).toBe(true);
    expect(matches(c, byFull, "featu")).toBe(true);
    expect(matches(c, byFull, "bcde")).toBe(false);
    expect(matches(c, byFull, "  ")).toBe(false);
  });
});

describe("context menus", () => {
  const ctx = { head: "c9", branch: "main" };

  it("start the wizards of strand G with their presets for a commit", () => {
    const menu = commitMenu(commit("c1"), refLabels([main.full, v1.full], byFull), ctx);
    expect(byId(menu, "branch-here").action).toEqual({ kind: "wizard", request: { kind: "create-branch", preset: { start: "c1" } } });
    expect(byId(menu, "tag-here").action).toEqual({ kind: "wizard", request: { kind: "create-tag", preset: { target: "c1" } } });
    expect(byId(menu, "cherry-pick").action).toEqual({ kind: "wizard", request: { kind: "cherry-pick", preset: { commit: "c1" } } });
    expect(byId(menu, "revert").action).toEqual({ kind: "wizard", request: { kind: "revert", preset: { commit: "c1" } } });
    expect(byId(menu, "reset")).toMatchObject({ label: "Move main here…", action: { kind: "wizard", request: { kind: "reset", preset: { target: "c1" } } } });
    expect(byId(menu, "compare-current").action).toEqual({ kind: "view", view: { kind: "compare", from: "c1", to: "HEAD" } });
    expect(byId(menu, "compare").action).toEqual({ kind: "view", view: { kind: "compare", from: "c1" } });
    expect(byId(menu, "copy-id").action).toEqual({ kind: "copy", text: "c1" });
    // One submenu per ref.
    expect(byId(menu, `ref:${main.full}`).items).toBeDefined();
    expect(byId(menu, `ref:${v1.full}`).icon).toBe("tag");
  });

  it("disable what makes no sense at HEAD or without a branch", () => {
    const atHead = commitMenu(commit("c9"), [], ctx);
    expect(byId(atHead, "cherry-pick").disabled).toBe(true);
    expect(byId(atHead, "reset").disabled).toBe(true);
    expect(byId(atHead, "compare-current").disabled).toBe(true);
    const detached = commitMenu(commit("c1"), [], { head: "c9", branch: null });
    expect(byId(detached, "reset")).toMatchObject({ disabled: true, hint: "Not on a branch" });
  });

  it("offers a branch here on a detached HEAD", () => {
    const head = ref("HEAD", "head", "c1", { current: true, name: "HEAD" });
    const menu = commitMenu(commit("c1"), [{ ref: head }], { head: "c1", branch: null });
    expect(items(menu)[0]).toMatchObject({ id: "detached", action: { kind: "wizard", request: { kind: "detached" } } });
  });

  it("start the branch, remote and tag wizards with their presets", () => {
    const f = refMenu(feature, ctx);
    expect(byId(f, "switch").action).toEqual({ kind: "wizard", request: { kind: "switch", preset: { target: "feature" } } });
    expect(byId(f, "merge")).toMatchObject({ label: "Merge into main…", action: { kind: "wizard", request: { kind: "merge", preset: { from: "feature" } } } });
    expect(byId(f, "publish").action).toEqual({ kind: "wizard", request: { kind: "publish", preset: { name: "feature" } } });
    expect(byId(f, "rename").action).toEqual({ kind: "wizard", request: { kind: "rename-branch", preset: { from: "feature" } } });
    expect(byId(f, "delete")).toMatchObject({ danger: true, action: { kind: "wizard", request: { kind: "delete-branch", preset: { name: "feature" } } } });
    expect(byId(f, "compare-current").action).toEqual({ kind: "view", view: { kind: "compare", from: "HEAD", to: "feature" } });

    const m = refMenu(main, ctx);
    expect(byId(m, "switch").disabled).toBe(true);
    expect(byId(m, "merge").disabled).toBe(true);
    expect(byId(m, "delete").disabled).toBe(true);
    expect(items(m).some((e) => e.id === "publish")).toBe(false);

    const r = refMenu(originOther, ctx);
    expect(byId(r, "switch").action).toEqual({ kind: "wizard", request: { kind: "switch", preset: { target: "origin/other" } } });
    expect(byId(r, "merge").action).toEqual({ kind: "wizard", request: { kind: "merge", preset: { from: "origin/other" } } });

    const t = refMenu(v1, ctx);
    expect(byId(t, "push-tag").action).toEqual({ kind: "wizard", request: { kind: "push-tag", preset: { name: "v1" } } });
    expect(byId(t, "delete-tag").action).toEqual({ kind: "wizard", request: { kind: "delete-tag", preset: { name: "v1" } } });
    expect(byId(t, "branch-here").action).toEqual({ kind: "wizard", request: { kind: "create-branch", preset: { start: "c1" } } });
  });
});

describe("selection and sync", () => {
  it("resolves a ref name to its commit", () => {
    expect(selectTarget("refs/heads/feature", [...byFull.values()])).toBe("c1");
    expect(selectTarget("v1", [...byFull.values()])).toBe("c1");
    expect(selectTarget("abc123", [...byFull.values()])).toBeNull();
  });

  it("says what a branch has to send and get in plain words", () => {
    expect(syncText(main)).toBe("2 to send, 1 to get (origin/main)");
    expect(syncText({ ...main, ahead: 0, behind: 0 })).toBe("In step with origin/main");
    expect(syncText(feature)).toBe("Not published yet");
  });
});

describe("semantic diff of one file", () => {
  it("keeps the changes of the file, also when the owner's root sits in a folder", () => {
    const change = (file: string) => ({ target: { kind: "request" as const, path: file }, change: { kind: "modified" as const }, file, name: file, fields: [] });
    const diff = { changes: [change("get.yml"), change("users/get.yml")], files: [{ path: "users/get.yml", status: "modified" as const, covered: false }] };
    expect(semanticFor(diff as never, "/api/users/get.yml")).toEqual({ changes: [diff.changes[1]], files: diff.files });
    expect(semanticFor(diff as never, "/get.yml").changes).toEqual([diff.changes[0]]);
    expect(semanticFor(diff as never, "/README.md")).toEqual({ changes: [], files: [] });
  });
});

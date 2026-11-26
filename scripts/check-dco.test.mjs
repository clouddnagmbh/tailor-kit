// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readCommits, unsigned } from "./check-dco.mjs";

function repo() {
  const dir = mkdtempSync(join(tmpdir(), "rt-dco-"));
  const git = (...args) => execFileSync("git", args, { cwd: dir, encoding: "utf8" }).trim();
  git("init", "-q", "-b", "main");
  git("config", "user.name", "Ada Example");
  git("config", "user.email", "ada@example.org");
  git("config", "commit.gpgsign", "false");
  const commit = (msg) => {
    git("commit", "-q", "--allow-empty", "-m", msg);
    return git("rev-parse", "HEAD");
  };
  return { dir, git, commit, done: () => rmSync(dir, { recursive: true, force: true }) };
}

test("signed-off commits pass, unsigned and foreign sign-offs fail", () => {
  const r = repo();
  try {
    const base = r.commit("base\n\nSigned-off-by: Ada Example <ada@example.org>");
    r.commit("good\n\nSigned-off-by: Ada Example <ADA@example.org>");
    const none = r.commit("no sign-off");
    const foreign = r.commit("wrong person\n\nSigned-off-by: Bob <bob@example.org>");
    const bad = unsigned(readCommits(`${base}..HEAD`, r.dir)).map((c) => c.sha);
    assert.deepEqual(bad.sort(), [none, foreign].sort());
  } finally {
    r.done();
  }
});

test("a single commit spec checks only that commit", () => {
  const r = repo();
  try {
    r.commit("unsigned parent");
    const head = r.commit("child\n\nSigned-off-by: Ada Example <ada@example.org>");
    const commits = readCommits(head, r.dir);
    assert.equal(commits.length, 1);
    assert.deepEqual(unsigned(commits), []);
  } finally {
    r.done();
  }
});

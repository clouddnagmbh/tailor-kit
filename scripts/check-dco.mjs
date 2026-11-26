// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// DCO gate: every non-merge commit needs "Signed-off-by: Name <email>" matching its author email.
// Usage: node scripts/check-dco.mjs <base>..<head>   (a range)
//        node scripts/check-dco.mjs <commit>         (just that commit)

import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const SIGN_OFF = /^Signed-off-by:\s*(.+?)\s*<([^>]+)>\s*$/gim;

/** Returns commits ({ sha, author, email, body }) in a range or a single commit. */
export function readCommits(spec, cwd = process.cwd()) {
  const args = ["log", "--no-merges", "--format=%H%x00%an%x00%ae%x00%B%x1e"];
  if (!spec.includes("..")) args.push("-1");
  args.push(spec);
  return execFileSync("git", args, { cwd, encoding: "utf8" })
    .split("\x1e")
    .map((r) => r.replace(/^\n/, ""))
    .filter((r) => r.trim() !== "")
    .map((r) => {
      const [sha, author, email, body] = r.split("\x00");
      return { sha, author, email, body };
    });
}

/** Returns the commits without a sign-off from their author. */
export function unsigned(commits) {
  return commits.filter((c) => {
    const emails = [...c.body.matchAll(SIGN_OFF)].map((m) => m[2].toLowerCase());
    return !emails.includes(c.email.toLowerCase());
  });
}

function main() {
  const spec = process.argv[2];
  if (!spec) {
    console.error("usage: check-dco.mjs <base>..<head> | <commit>");
    process.exit(2);
  }
  const commits = readCommits(spec);
  const bad = unsigned(commits);
  if (bad.length > 0) {
    console.error("DCO check FAILED. These commits lack a matching Signed-off-by line:");
    for (const c of bad) console.error(`  ${c.sha.slice(0, 12)} ${c.author} <${c.email}>`);
    console.error("Fix with: git rebase --signoff <base>   (see CONTRIBUTING.md)");
    process.exit(1);
  }
  console.log(`DCO ok: ${commits.length} commit(s) signed off`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}

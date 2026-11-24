// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand F: the index of the mock's Git (`GitApi.indexDiff`, `stage`, `unstage`,
// `stagePatch`, `unstagePatch`, `discardPatch`, `commitStaged`, `verify`, `config`).
//
// The mock keeps a text per file (`blob`): the index is a map path → staged text (or `null`
// for a staged deletion); a path without an entry is "as in HEAD". Line selection is the same
// code the web app uses (`line-selection.ts`). The mock's commits are request-based, so
// `commitStaged` commits whole request files only: it refuses a request file that is staged
// in part, and files that are not requests stay staged. Nothing is signed: `verify` says
// `unsigned`. `discardPatch` changes the file as the index views see it (the rest of the
// mock's working tree is read-only).

import { BackendError } from "./backend";
import { applyLineSelection, chosenLines } from "./line-selection";
import { lineDiff, type MockGit } from "./mock-git";
import type { GitCommit, GitDiff, GitIndexDiff, GitPatchRequest, GitPatchSide, GitSignatureCheck } from "./types";
import type { GitIndexMethods } from "./wave15-stubs";

const err = (message: string) => new BackendError("git", message);

/** Values `GitApi.config` answers in the mock (tests set them). */
export const MOCK_GIT_CONFIG = new Map<string, string>();

/** A made-up blob id for a text (`""` when the file is missing). */
export function mockBlobId(text: string | null): string {
  if (text === null) return "";
  let h = 5381;
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) >>> 0;
  return `mock${h.toString(16).padStart(8, "0")}${text.length.toString(16)}`;
}

/** A file the viewers cannot show as text. */
const BINARY = "\u0000binary";

const slash = (p: string) => (p.startsWith("/") ? p : `/${p}`);

export function mockGitIndex(git: MockGit): GitIndexMethods {
  /** Staged text by path; `null`: staged deletion. */
  const index = new Map<string, string | null>();
  /** Working files `discardPatch` changed (the mock's working tree is read-only otherwise). */
  const discarded = new Map<string, string | null>();

  async function textAt(rev: string | null, path: string): Promise<string | null> {
    if (rev === null && discarded.has(path)) return discarded.get(path)!;
    if (rev !== null && (await git.log(1)).length === 0) return null;
    try {
      const blob = await git.blob(rev, path);
      return blob.text ?? BINARY;
    } catch {
      return null;
    }
  }

  async function sides(path: string, side: GitPatchSide): Promise<{ old: string | null; new: string | null }> {
    const p = slash(path);
    const head = await textAt("HEAD", p);
    const staged = index.has(p) ? index.get(p)! : head;
    return side === "unstaged" ? { old: staged, new: await textAt(null, p) } : { old: head, new: staged };
  }

  function diffOf(path: string, old: string | null, now: string | null): GitDiff {
    if (old === BINARY || now === BINARY) return { path: slash(path), binary: true, hunks: [] };
    return lineDiff(slash(path), old, now);
  }

  /** The text a selection of `patch` produces; refuses a stale one. */
  async function selected(patch: GitPatchRequest, side: GitPatchSide, invert: boolean) {
    if (patch.side !== side) throw err("This change works on the other side of the file.");
    const s = await sides(patch.path, side);
    if (mockBlobId(s.old) !== patch.old || mockBlobId(s.new) !== patch.new) {
      throw err(`${slash(patch.path)} changed since you chose these lines: reload it and choose again.`);
    }
    if (s.old === BINARY || s.new === BINARY) throw err(`${slash(patch.path)} is not a text file: choose it as a whole.`);
    const diff = diffOf(patch.path, s.old, s.new);
    const chosen = chosenLines(diff, patch.hunks);
    const text = applyLineSelection(s.old ?? "", s.new ?? "", diff, chosen, invert);
    return { s, chosen, text, p: slash(patch.path) };
  }

  return {
    async indexDiff(path, side): Promise<GitIndexDiff> {
      const s = await sides(path, side);
      return { side, diff: diffOf(path, s.old, s.new), old: mockBlobId(s.old), new: mockBlobId(s.new) };
    },

    async stage(paths): Promise<void> {
      for (const raw of paths) {
        const p = slash(raw);
        const work = await textAt(null, p);
        if (work === null && (await textAt("HEAD", p)) === null && !index.has(p)) {
          throw err(`${p} is neither in the working tree nor staged.`);
        }
        index.set(p, work);
      }
    },

    async unstage(paths): Promise<void> {
      for (const raw of paths) index.delete(slash(raw));
    },

    async stagePatch(patch): Promise<void> {
      const { s, chosen, text, p } = await selected(patch, "unstaged", false);
      if (chosen.size === 0) return;
      index.set(p, s.new === null && text === "" ? null : text);
    },

    async unstagePatch(patch): Promise<void> {
      const { s, chosen, text, p } = await selected(patch, "staged", true);
      if (chosen.size === 0) return;
      if (text === s.old || (s.old === null && text === "")) index.delete(p);
      else index.set(p, text);
    },

    async discardPatch(patch): Promise<void> {
      const { s, chosen, text, p } = await selected(patch, "unstaged", true);
      if (chosen.size === 0) return;
      if (s.old === null) throw err(`${p} is not tracked yet: there is nothing to go back to.`);
      discarded.set(p, text);
    },

    async commitStaged(commit): Promise<GitCommit> {
      const status = await git.status();
      const requests = new Map(status.files.map((f) => [`/${f.path}`, f.path]));
      const paths: string[] = [];
      for (const [p, text] of index) {
        const request = requests.get(p);
        if (!request) continue;
        if (text !== (await textAt(null, p))) {
          throw err(`${p} is staged in part: the mock commits whole request files. Stage the whole file.`);
        }
        paths.push(request);
      }
      if (paths.length === 0) throw err("Nothing is staged.");
      const done = await git.commit(commit.message, paths);
      for (const p of paths) index.delete(slash(p));
      return done;
    },

    async verify(oids): Promise<GitSignatureCheck[]> {
      const out: GitSignatureCheck[] = [];
      for (const oid of oids) out.push({ oid: await git.revParse(oid), state: "unsigned" });
      return out;
    },

    async config(key): Promise<string | null> {
      return MOCK_GIT_CONFIG.get(key) ?? null;
    },
  };
}

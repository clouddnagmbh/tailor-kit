// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// Wave 15, strand B: the working copy, file tree and file contents of the mock's Git
// (`GitApi.workingCopy`, `fileTree`, `blob`), from `MockGit`'s public methods.
//
// The repository the mock stands for holds the product's modelled files (`MockGit` says which
// changed) and, next to them, the files of `MockFixtures.extraFiles`, so the viewers have
// something to show (text, an image that changed, a binary, a Git LFS pointer, a file deleted in
// the working tree, a new note, an ignored log). They exist from the first commit on and only
// change in the working tree; every revision shows the committed version.

import { BackendError } from "../api";
import type { MockExtraFile } from "./content";
import type { MockGit } from "./git";
import type { GitBlob, GitFileState, GitTreeEntry, GitWorkingCopy, GitWorkingFile } from "../types";
import type { GitWorkingMethods } from "../git-methods";

interface Extra extends MockExtraFile {}

const LFS_MARK = "version https://git-lfs.github.com/spec/v1";

const SIZE = (e: Extra): number => (e.binary ?? (e.base64 ? Math.floor((e.base64.length * 3) / 4) : (e.text?.length ?? 0)));

function blobOf(path: string, rev: string | undefined, extra: Extra): GitBlob {
  const lfs = extra.text?.startsWith(LFS_MARK) ?? false;
  const image = extra.media?.startsWith("image/") ?? false;
  return {
    path,
    ...(rev ? { rev } : {}),
    oid: `mock${[...path].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 1_000_003, 7).toString(16).padStart(8, "0")}${SIZE(extra).toString(16)}`,
    size: SIZE(extra),
    binary: extra.binary !== undefined || image,
    ...(extra.media ? { media_type: extra.media } : {}),
    ...(extra.text !== undefined ? { text: extra.text } : {}),
    ...(extra.base64 ? { base64: extra.base64 } : {}),
    truncated: false,
    lfs_pointer: lfs,
  };
}

const NOT_REPO = () => new BackendError("git", "This folder is not a Git repository.");

export function mockGitFiles<S>(git: MockGit<S>): GitWorkingMethods {
  const { fixtures } = git;
  const COMMITTED = fixtures.extraFiles.committed;
  const WORKING = fixtures.extraFiles.working;
  const IGNORED = new Set(fixtures.extraFiles.ignored);
  async function committed(): Promise<boolean> {
    return (await git.log(1)).length > 0;
  }

  /** The files of the working tree and of `HEAD`: request files by `MockGit`, the rest above. */
  async function snapshot(): Promise<{ head: Map<string, Extra>; work: Map<string, Extra>; states: Map<string, GitFileState> }> {
    const status = await git.status();
    if (!status.repo) throw NOT_REPO();
    const head = new Map<string, Extra>();
    const work = new Map<string, Extra>();
    const states = new Map<string, GitFileState>();
    if (await committed()) for (const [p, e] of Object.entries(COMMITTED)) head.set(p, e);
    for (const [p, e] of Object.entries(COMMITTED)) work.set(p, e);
    for (const [p, e] of Object.entries(WORKING)) {
      if (e === null) work.delete(p);
      else work.set(p, e);
    }
    if (!(await committed())) for (const p of work.keys()) states.set(p, "untracked");
    else {
      for (const [p, e] of work) {
        const old = head.get(p);
        if (!old) states.set(p, "untracked");
        else if (JSON.stringify(old) !== JSON.stringify(e)) states.set(p, "modified");
      }
      for (const p of head.keys()) if (!work.has(p)) states.set(p, "deleted");
    }
    for (const p of IGNORED) states.delete(p);
    for (const f of status.files) states.set(`/${f.path}`, f.state);
    return { head, work, states };
  }

  return {
    workingCopy: async (): Promise<GitWorkingCopy> => {
      const status = await git.status();
      if (!status.repo) throw NOT_REPO();
      const { states } = await snapshot();
      const requests = new Set(status.files.map((f) => `/${f.path}`));
      const files: GitWorkingFile[] = [...states]
        .map(([path, state]): GitWorkingFile => ({
          path,
          state,
          unstaged: state !== "conflicted",
          ...(requests.has(path) ? fixtures.tag(path.slice(1), "working") : {}),
        }))
        .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
      const hasHead = await committed();
      return {
        repository: fixtures.repository,
        branch: status.branch,
        upstream: status.upstream,
        ahead: status.ahead,
        behind: status.behind,
        ...(hasHead ? { head: await git.revParse("HEAD") } : {}),
        detached: hasHead && status.branch === null,
        ...(status.merging ? { operation: "merge" as const } : {}),
        files,
        truncated: false,
      };
    },

    fileTree: async (rev, dir): Promise<GitTreeEntry[]> => {
      const { head, work, states } = await snapshot();
      const base = dir === "" || dir === "/" ? "" : `/${dir.replace(/^\/|\/$/g, "")}`;
      // Every file path of the side (request files from `MockGit`).
      const paths = new Map<string, number>();
      if (rev === null) {
        for (const [p, e] of work) paths.set(p, SIZE(e));
        for (const [p, s] of states) if (s !== "deleted" && !paths.has(p)) paths.set(p, 1);
        for (const [p, s] of states) if (s === "deleted") paths.set(p, 0);
      } else {
        for (const [p, e] of head) paths.set(p, SIZE(e));
        for (const p of Object.keys((await git.treeAt(rev)).files)) paths.set(`/${p}`, paths.get(`/${p}`) ?? 1);
      }
      const out = new Map<string, GitTreeEntry>();
      for (const [path, size] of paths) {
        if (!path.startsWith(`${base}/`)) continue;
        const rest = path.slice(base.length + 1);
        const slash = rest.indexOf("/");
        const name = slash < 0 ? rest : rest.slice(0, slash);
        const entryPath = `${base}/${name}`;
        if (slash >= 0) {
          out.set(entryPath, { name, path: entryPath, kind: "dir", ignored: false });
        } else {
          const state = rev === null ? states.get(path) : undefined;
          out.set(entryPath, {
            name,
            path: entryPath,
            kind: "file",
            ...(state === "deleted" ? {} : { size }),
            ...(state ? { state } : {}),
            ignored: rev === null && IGNORED.has(path),
          });
        }
      }
      return [...out.values()].sort((a, b) => {
        const folder = (e: GitTreeEntry) => (e.kind === "dir" ? 0 : 1);
        return folder(a) - folder(b) || a.name.toLowerCase().localeCompare(b.name.toLowerCase());
      });
    },

    blob: async (rev, path): Promise<GitBlob> => {
      const { head, work } = await snapshot();
      const at = path.startsWith("/") ? path : `/${path}`;
      const commit = rev === null ? undefined : await git.revParse(rev);
      const extra = (rev === null ? work : head).get(at);
      if (extra) return blobOf(at, commit, extra);
      const text = rev === null ? git.workText(at) : await git.show(rev, at);
      if (text === null) {
        throw new BackendError("git", rev === null ? `${at} does not exist in the working tree.` : `${at} does not exist at ${rev}.`);
      }
      return blobOf(at, commit, { text });
    },
  };
}

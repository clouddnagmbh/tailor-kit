// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15, strand B: the working copy, file tree and file contents of the mock's Git
// (`GitApi.workingCopy`, `fileTree`, `blob`), from `MockGit`'s public methods.
//
// The mock's repository root is the demo collection, so every request file is a collection
// file (the status of `MockGit` says which changed). Next to them the mock repository holds a
// few other files, so the viewers have something to show: a README and a guide (text), a logo
// (an image that changed), a binary file, a Git LFS pointer, a file deleted in the working tree,
// a new note and an ignored log. They exist from the first commit on and only change in the
// working tree; every revision shows the committed version.

import { BackendError } from "./backend";
import { DEMO_ID } from "./mock-collections";
import type { MockGit } from "./mock-git";
import type { GitBlob, GitFileState, GitTreeEntry, GitWorkingCopy, GitWorkingFile } from "./types";
import type { GitWorkingMethods } from "./wave15-stubs";

/** The id of the mock repository (`mock-collections.ts`). */
export const MOCK_REPOSITORY = "mock://demo";

/** 24 × 16 px blue, and the same with an orange square (valid PNGs). */
export const LOGO_OLD =
  "iVBORw0KGgoAAAANSUhEUgAAABgAAAAQCAIAAACDRijCAAAAGklEQVR42mPQiDpBFcQwatCoQaMGjRpEGQIAjqjvEGSqPNsAAAAASUVORK5CYII=";
export const LOGO_NEW =
  "iVBORw0KGgoAAAANSUhEUgAAABgAAAAQCAIAAACDRijCAAAAIElEQVR42mPQiDpBFcQwogz60CNCEI0aNGrQCM4i5CEAArsJX6x4MK4AAAAASUVORK5CYII=";

interface Extra {
  text?: string;
  base64?: string;
  media?: string;
  /** Binary without a viewer: its size. */
  binary?: number;
}

const LFS = (oid: string, size: number) => `version https://git-lfs.github.com/spec/v1\noid sha256:${oid}\nsize ${size}\n`;

const COMMITTED: Record<string, Extra> = {
  "/.gitignore": { text: "*.log\n" },
  "/README.md": { text: "# Demo API\n\nRequests for the demo service.\nSee the docs folder for the guides.\n" },
  "/docs/guide.md": { text: "# Guide\n\nSend a request with Mod+Enter.\n" },
  "/assets/logo.png": { base64: LOGO_OLD, media: "image/png" },
  "/assets/model.glb": { text: LFS("1f2e3d4c5b6a79881f2e3d4c5b6a79881f2e3d4c5b6a79881f2e3d4c5b6a7988", 4_194_304) },
  "/data/blob.bin": { binary: 2048 },
  "/legacy/old.txt": { text: "Replaced by docs/guide.md.\n" },
};

/** What the working tree holds instead: `null` deletes the file. */
const WORKING: Record<string, Extra | null> = {
  "/README.md": { text: "# Demo API\n\nRequests for the demo service.\nSee the guides folder for the walkthroughs.\n" },
  "/assets/logo.png": { base64: LOGO_NEW, media: "image/png" },
  "/assets/model.glb": { text: LFS("9a8b7c6d5e4f30219a8b7c6d5e4f30219a8b7c6d5e4f30219a8b7c6d5e4f3021", 5_242_880) },
  "/data/blob.bin": { binary: 4096 },
  "/legacy/old.txt": null,
  "/notes/todo.md": { text: "- document the retry policy\n- add a request for /health\n" },
  "/build.log": { text: "built in 3.2 s\n" },
};

const IGNORED = new Set(["/build.log"]);

const SIZE = (e: Extra): number => (e.binary ?? (e.base64 ? Math.floor((e.base64.length * 3) / 4) : (e.text?.length ?? 0)));

function blobOf(path: string, rev: string | undefined, extra: Extra): GitBlob {
  const lfs = extra.text?.startsWith("version https://git-lfs.github.com/spec/v1") ?? false;
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

export function mockGitFiles(git: MockGit): GitWorkingMethods {
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
          ...(requests.has(path) ? { collection: DEMO_ID } : {}),
        }))
        .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
      const hasHead = await committed();
      return {
        repository: MOCK_REPOSITORY,
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

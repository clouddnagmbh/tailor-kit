// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15 (interface): the method groups the Git client adds to `GitApi`, one per strand, and
// placeholders that reject with kind `unsupported` ("not implemented yet") until the strand
// replaces them in its own files (desktop `src/<group>.ts`, web `src/git/<group>.ts`, mock
// `commands/mock-<group>.ts`). Exported from the package so the shells can use them until then.

import type { GitApi, ReviewApi } from "./backend";
import { BackendError } from "./backend";

/** Strand B: working copy, file tree, file contents. */
export type GitWorkingMethods = Pick<GitApi, "workingCopy" | "fileTree" | "blob">;
/** Strand C: blame, history search, reflog. */
export type GitHistoryMethods = Pick<GitApi, "blame" | "searchHistory" | "reflog">;
/** Strand D: building blocks for history editors and the text conflict result. */
export type GitPlumbingMethods = Pick<GitApi, "commitInfo" | "pick" | "writeTree" | "writeCommit" | "resolveText">;
/** Strand F: the index and signatures. */
export type GitIndexMethods = Pick<
  GitApi,
  "indexDiff" | "stage" | "unstage" | "stagePatch" | "unstagePatch" | "discardPatch" | "commitStaged" | "verify"
>;

function notYet(feature: string, strand: string): () => Promise<never> {
  return () => Promise.reject(new BackendError("unsupported", `${feature} is not implemented yet (wave 15, strand ${strand}).`));
}

export function stubGitWorking(): GitWorkingMethods {
  const no = notYet("The working copy of any repository", "B");
  return { workingCopy: no, fileTree: no, blob: no };
}

export function stubGitHistory(): GitHistoryMethods {
  const no = notYet("Blame, history search and the reflog", "C");
  return { blame: no, searchHistory: no, reflog: no };
}

export function stubGitPlumbing(): GitPlumbingMethods {
  const no = notYet("Commit building blocks", "D");
  return { commitInfo: no, pick: no, writeTree: no, writeCommit: no, resolveText: no };
}

export function stubGitIndex(): GitIndexMethods {
  const no = notYet("Index operations", "F");
  return {
    indexDiff: no,
    stage: no,
    unstage: no,
    stagePatch: no,
    unstagePatch: no,
    discardPatch: no,
    commitStaged: no,
    verify: no,
  };
}

/** `ReviewApi.create` until strand E. */
export const stubCreatePull: ReviewApi["create"] = notYet("Opening pull requests", "E");

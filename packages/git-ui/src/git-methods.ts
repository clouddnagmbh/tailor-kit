// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 15: the method groups the Git client adds to `GitApi`, one per area. Every shell implements
// them itself (desktop `src/<group>.ts`, web `src/git/<group>.ts`, mock `commands/mock-<group>.ts`);
// the names only let each shell type its own file.

import type { GitApi } from "./api";

/** Strand B: working copy, file tree, file contents. */
export type GitWorkingMethods = Pick<GitApi, "workingCopy" | "fileTree" | "blob">;
/** Strand C: blame, history search, reflog. */
export type GitHistoryMethods = Pick<GitApi, "blame" | "searchHistory" | "reflog">;
/** Strand D: building blocks for history editors and the text conflict result. */
export type GitPlumbingMethods = Pick<GitApi, "commitInfo" | "pick" | "writeTree" | "writeCommit" | "resolveText">;
/** Strand F: the index and signatures. */
export type GitIndexMethods = Pick<
  GitApi,
  "indexDiff" | "stage" | "unstage" | "stagePatch" | "unstagePatch" | "discardPatch" | "commitStaged" | "verify" | "config"
>;

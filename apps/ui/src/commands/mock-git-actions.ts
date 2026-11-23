// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand G: the wizards' actions on the mock's Git (`GitApi.plan`, `apply`, `undo`,
// remotes, stashes). Strand G owns `mock-git.ts` too and adds the state it needs there (tags,
// remotes, stashes, journal). Placeholder until strand G.

import type { MockGit } from "./mock-git";
import { stubGitActions, type GitActionMethods } from "./wave14-stubs";

export function mockGitActions(git: MockGit): GitActionMethods {
  void git;
  return stubGitActions();
}

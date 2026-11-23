// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: the commit graph of the mock's Git (`GitApi.graph`, `refs`, `changes`,
// `fileDiff`), from `MockGit.graphData()` (copies, never the private fields). Placeholder until
// strand F.

import type { MockGit } from "./mock-git";
import { stubGitGraph, type GitGraphMethods } from "./wave14-stubs";

export function mockGitGraph(git: MockGit): GitGraphMethods {
  void git;
  return stubGitGraph();
}

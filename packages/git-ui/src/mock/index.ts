// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// @tailor-kit/git-ui/mock: an in-memory Git, a repository set and a review host behind the
// `GitApi`, `GitRepositoryApi` and `ReviewApi` interfaces, for tests and for a product's browser
// harness (no backend needed). Moved from RequestTailor's `apps/ui/src/commands/mock-git*.ts` in
// step K3. The product decides what a file is (`MockContent`) and brings its fixtures
// (`MockFixtures`); the defaults are plain text files.

export * from "./content";
export { MOCK_REMOTE_HOST, MOCK_REMOTE_URL, MockGit, lineDiff, mockGit, type Commit, type MockGitHost, type MockGitOptions, type MockScenarioCore, type Tree } from "./git";
export { mergeTrees, type Core, type MockGitActions, type MockJournalEntry, type MockStash } from "./actions";
export { MOCK_GIT_CONFIG, mockBlobId } from "./staging";
export { mockRefusal, mockScan } from "./secret-scan";
export { MOCK_REVIEW_TOKEN, mockReview, textReviewOptions, type MockReviewOptions } from "./review";
export { mockRepositories, type MockRepositories } from "./repositories";

import type { GitHostBackend } from "../api";
import { MockGit, type MockGitOptions } from "./git";
import { mockRepositories, type MockRepositories } from "./repositories";
import { mockReview, textReviewOptions } from "./review";

/** The whole mock `GitHostBackend` over plain text files: `git`, a repository set and a review
 *  host, `kind` `"mock"`. For a product's browser harness and for tests of the Git views. */
export interface MockGitBackend extends GitHostBackend {
  readonly git: MockGit;
  readonly repositories: MockRepositories;
}

export function createMockGitBackend(options: MockGitOptions = {}): MockGitBackend {
  const git = new MockGit(options);
  return {
    git,
    repositories: mockRepositories(() => git),
    review: mockReview(textReviewOptions(() => git, () => git.savedFiles())),
    kind: "mock",
  };
}

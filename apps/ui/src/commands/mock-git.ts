// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Git in the mock backend, for the Git UI's tests and UI development. The engine (commits,
// branches, a remote, merges, the wizards' actions, the graph, files, history, the index) lives
// in `@tailor-kit/git-ui/mock` (step K3); this file binds it to RequestTailor: a file is a
// request spec (`mock-git-content.ts`), the working tree is the mock backend's saved requests
// (`MockGitHost.saved`), and the fixtures are the demo collection.

import { MockGit as KitMockGit, type MockGitHost as KitMockGitHost } from "@tailor-kit/git-ui/mock";

import type { CollectionInfo } from "./backend";
import { specContent, specFixtures } from "./mock-git-content";
import type { RequestSpec } from "./types";

export {
  lineDiff,
  MOCK_GIT_CONFIG,
  MOCK_REMOTE_HOST,
  MOCK_REMOTE_URL,
  type MockGitScenario,
} from "@tailor-kit/git-ui/mock";
export { renderSpec } from "./mock-git-content";

/** What the mock Git needs from the mock backend. */
export interface MockGitHost {
  /** Requests in memory (the editor state). */
  requests(): Map<string, RequestSpec>;
  /** Requests as saved "on disk": the working tree. */
  saved(): Map<string, RequestSpec>;
  dirty(): Promise<string[]>;
  openCollection(): Promise<CollectionInfo | null>;
}

/** The mock's Git, with test helpers beyond `GitApi`. */
export class MockGit extends KitMockGit<RequestSpec> {
  constructor(host?: MockGitHost, clock?: () => number) {
    const kitHost: KitMockGitHost<RequestSpec> | undefined = host && {
      live: () => host.requests(),
      saved: () => host.saved(),
      dirty: () => host.dirty(),
      openOwner: () => host.openCollection(),
    };
    super({ host: kitHost, clock, content: specContent, fixtures: specFixtures });
  }

  /** The request `path` at `rev` as a model, `null` if it is not there. */
  specAt(rev: string, path: string): RequestSpec | null {
    return this.fileAt(rev, path);
  }

  /** Every request at `rev` as a model, by path. */
  specsAt(rev: string): Map<string, RequestSpec> {
    return this.filesAt(rev);
  }
}

export function mockGit(host?: MockGitHost, clock?: () => number): MockGit {
  return new MockGit(host, clock);
}

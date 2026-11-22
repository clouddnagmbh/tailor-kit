// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Git in the mock backend, for the Git UI's tests. Owner: wave 4, strand A2.
// Until then: "not a repository", and every other command is unsupported.

import { BackendError, type GitApi } from "./backend";
import type { GitStatus } from "./types";

const unsupported = async (): Promise<never> => {
  throw new BackendError("unsupported", "Git is not available in the mock backend yet.");
};

export function mockGit(): GitApi {
  return {
    status: async (): Promise<GitStatus> => ({
      repo: false,
      branch: null,
      upstream: null,
      ahead: 0,
      behind: 0,
      files: [],
      merging: false,
    }),
    diff: unsupported,
    commit: unsupported,
    log: async () => [],
    branches: async () => [],
    checkout: unsupported,
    createBranch: unsupported,
    push: unsupported,
    pull: unsupported,
    conflict: unsupported,
    resolve: unsupported,
    init: unsupported,
    clone: unsupported,
    identity: async () => null,
    setIdentity: unsupported,
    hasCredentials: async () => false,
    setCredentials: unsupported,
    deleteCredentials: unsupported,
  };
}

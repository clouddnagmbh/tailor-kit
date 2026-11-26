// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// A `GitHost` for tests, built on the mock backend: no product state, no `content` unless a test
// passes one. It records what the views ask of the host.

import { MockBackend } from "../commands/mock";
import type { GitBrand, GitContent, GitDialog, GitHost } from "./host";
import type { GitView } from "./views.svelte";

export const TEST_BRAND: GitBrand = { productName: "TestTailor", commitMime: "application/x-testtailor-commit", refNamespace: "testtailor" };

export interface TestGitHost extends GitHost {
  /** What the views asked for, in order. */
  readonly calls: { shown: GitView[]; dialogs: GitDialog[]; errors: string[]; reloads: number };
}

export interface TestGitHostOptions {
  backend?: MockBackend;
  content?: GitContent;
  activeRepository?: string | null;
  unsaved?: string[];
}

export function testGitHost(options: TestGitHostOptions = {}): TestGitHost {
  const calls = { shown: [] as GitView[], dialogs: [] as GitDialog[], errors: [] as string[], reloads: 0 };
  const host: TestGitHost = {
    backend: options.backend ?? new MockBackend(),
    brand: TEST_BRAND,
    activeRepository: options.activeRepository ?? null,
    repositories: [],
    reloadCount: 0,
    cacheScope: null,
    commitPrefill: null,
    dialogOpen: false,
    unsaved: () => options.unsaved ?? [],
    reload: async () => {
      calls.reloads++;
    },
    show: (view) => void calls.shown.push(view),
    showDialog: (dialog) => {
      calls.dialogs.push(dialog);
      return Promise.resolve(undefined);
    },
    takeCommitPrefill: () => null,
    adoptRepository: async () => {},
    notifyError: (message) => void calls.errors.push(message),
    commands: () => [],
    content: options.content,
    calls,
  };
  return host;
}

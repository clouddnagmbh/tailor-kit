// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// A `GitHost` for tests: no product state, no `content` and no extensions unless a test passes
// them, and a backend that refuses every call unless a test passes one. It records what the views
// ask of the host.

import { createShellStorage, HintStore } from "@tailor-kit/shell";

import type { GitHostBackend } from "./api";
import type { ProvidedGitExtensions } from "./extensions";
import type { GitBrand, GitContent, GitDialog, GitHost } from "./host";
import type { GitView } from "./views.svelte";

export const TEST_BRAND: GitBrand = { productName: "TestTailor", commitMime: "application/x-testtailor-commit", refNamespace: "testtailor" };

export interface TestGitHost extends GitHost {
  /** What the views asked for, in order. */
  readonly calls: { shown: GitView[]; dialogs: GitDialog[]; errors: string[]; reloads: number };
}

/** A backend whose every call throws: for tests that never reach it. */
function unusedBackend(): GitHostBackend {
  const refuse = (): never => {
    throw new Error("testGitHost: no backend given");
  };
  const group = () => new Proxy({}, { get: () => refuse });
  return { git: group() as never, repositories: group() as never, review: group() as never, kind: "mock" };
}

export interface TestGitHostOptions {
  backend?: GitHostBackend;
  extensions?: ProvidedGitExtensions;
  content?: GitContent;
  activeRepository?: string | null;
  unsaved?: string[];
}

export function testGitHost(options: TestGitHostOptions = {}): TestGitHost {
  const calls = { shown: [] as GitView[], dialogs: [] as GitDialog[], errors: [] as string[], reloads: 0 };
  const storage = createShellStorage("test.shell.");
  const host: TestGitHost = {
    backend: options.backend ?? unusedBackend(),
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
    extensions: options.extensions ?? null,
    storage,
    hints: new HintStore(storage),
    content: options.content,
    calls,
  };
  return host;
}

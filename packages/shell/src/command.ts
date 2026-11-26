// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

/** One entry of the command palette. */
export interface Command {
  /** Unique within the list. */
  id: string;
  label: string;
  /** Display form of the shortcut, if any. */
  shortcut?: string;
  /** Marks the current choice (for example the active environment). */
  checked?: boolean;
  run: () => unknown;
}

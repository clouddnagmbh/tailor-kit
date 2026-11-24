// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand G: palette entries for every Git wizard, "Undo last Git action" and
// "Git: History" (the graph). The same wizards are reachable from the Git panel (branch chip,
// the repository card's menu, the changes menu, each commit's menu); the palette is the
// keyboard path to them.

import type { Command } from "../../shell/commands";
import type { AppState } from "../../state/app.svelte";
import type { GitViews } from "../views.svelte";
import { openGitWizard, wizardTitle, type GitWizardKind } from "./open";

/** Palette ids start with this prefix; the panel's tool tiles leave them out. */
export const WIZARD_COMMAND_PREFIX = "git-wizard:";

/** The wizards in the palette, in the order people look for them. */
const PALETTE_WIZARDS: GitWizardKind[] = [
  "create-branch",
  "switch",
  "merge",
  "pull",
  "push",
  "publish",
  "fetch",
  "stash",
  "amend",
  "abort",
  "unstash",
  "drop-stash",
  "revert",
  "cherry-pick",
  "reset",
  "discard",
  "create-tag",
  "delete-tag",
  "push-tag",
  "rename-branch",
  "delete-branch",
  "remotes",
  "add-remote",
  "clone",
];

/** Opens a wizard; it reloads the collections itself when the action changed files. */
async function run(app: AppState, kind: GitWizardKind): Promise<void> {
  await openGitWizard(app, { kind });
}

export function gitWizardCommands(app: AppState, views: GitViews): Command[] {
  if (!app.collection && !app.activeRepository) return [];
  return [
    { id: `${WIZARD_COMMAND_PREFIX}history`, label: "Git: History", run: () => views.open({ kind: "graph" }) },
    { id: `${WIZARD_COMMAND_PREFIX}undo`, label: "Git: Undo last Git action", run: () => run(app, "undo") },
    ...PALETTE_WIZARDS.map((kind) => ({ id: `${WIZARD_COMMAND_PREFIX}${kind}`, label: `Git: ${wizardTitle(kind)}…`, run: () => run(app, kind) })),
  ];
}

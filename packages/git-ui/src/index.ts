// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// @tailor-kit/git-ui: the Git views (commit graph, changes, files, history, wizards, conflicts,
// pull-request review) behind a `GitHost`. Provenance: RequestTailor's `apps/ui/src/git`,
// `review` and `diff/LineDiff`, moved here in step K3. A product implements `GitHost` and its
// `GitHostBackend`, puts the host in place with `setGitHost` and the views with `provideGitViews`,
// then mounts `GitViewHost` and `GitPanel`.

export type * from "./types";
export type * from "./api";
export { BackendError } from "./api";
export type * from "./extensions";

export { default as ExtensionActions } from "./ExtensionActions.svelte";
export { default as ExtensionGitActions } from "./ExtensionGitActions.svelte";
export { default as ExtensionGitHint } from "./ExtensionGitHint.svelte";
export { default as GitPanel } from "./GitPanel.svelte";
export { default as GitViewHost } from "./GitViewHost.svelte";
export { default as CloneForm } from "./CloneForm.svelte";
export { default as CloneRepositoryDialog } from "./CloneRepositoryDialog.svelte";
export { default as LineDiff } from "./diff/LineDiff.svelte";
export { default as Words } from "./diff/Words.svelte";

export * from "./clone-source";
export * from "./extension-git";
export * from "./format";
export * from "./glossary";
export * from "./host";
export * from "./views.svelte";

export { lineDiff, lineOps, type LineOp } from "./diff/textdiff";
export { textRows, tokens, wordDiff, type Segment } from "./diff/worddiff";
export { dateOrder, graphPage, reachable, sortChanges, sortRefs, type GraphSource, type OrderCommit } from "./graph/order";
export { historyCommands } from "./history/commands";
export { CI_LABEL, DEVICE_LOGIN_REGISTERED, deviceLoginOffered, groupComments, relativeTime, requestWord, stateLabel } from "./review/model";
export { reviewCommands } from "./review/commands";
export { openNewPull } from "./review/open-new-pull";
export { gitWizardCommands } from "./wizards/commands";
export { openGitWizard } from "./wizards/open";
export { refNameProblem, suggestRefName } from "./wizards/refname";
export { syntheticRepo } from "./graph/synthetic";

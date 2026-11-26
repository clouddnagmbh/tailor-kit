// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

export { default as CommandPalette } from "./CommandPalette.svelte";
export { default as DialogHost } from "./DialogHost.svelte";
export { default as Resizer } from "./Resizer.svelte";
export type { Command } from "./command";
export type { HostedDialog } from "./dialog";
export { fuzzyFilter, fuzzyScore } from "./fuzzy";
export { HintStore } from "./hint-store";
export { formatShortcut, isMacPlatform, matchShortcut, type Shortcut } from "./shortcuts";
export { createShellStorage, isNumber, type ShellStorage } from "./storage";
export { fitTabs, TAB_MAX, TAB_MIN, TAB_MORE, type TabFit, type TabFitInput } from "./tab-fit";
export { Theme, type ThemePreference } from "./theme.svelte";

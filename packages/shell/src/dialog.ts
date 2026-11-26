// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

/** What the dialog host needs to know about the dialog it shows. */
export interface HostedDialog {
  /** Counts dialogs; a new value remounts the content. */
  seq: number;
  title: string;
  /** Width of the frame (`Dialog`'s sizes or a pixel width); default `md`. */
  width?: "sm" | "md" | "lg" | "xl" | number;
}

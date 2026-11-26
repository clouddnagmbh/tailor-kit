// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Light, dark or the OS preference (the shared `Theme` of @tailor-kit/shell), remembered under
// RequestTailor's storage keys.

import { Theme as KitTheme } from "@tailor-kit/shell";

import { shellStorage } from "./storage";

export type { ThemePreference } from "@tailor-kit/shell";

export class Theme extends KitTheme {
  constructor(root?: HTMLElement) {
    super(shellStorage, root ?? globalThis.document?.documentElement);
  }
}

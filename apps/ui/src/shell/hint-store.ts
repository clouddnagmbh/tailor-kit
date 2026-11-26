// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Which extension hints the user dismissed (wave 14, strand C): the shared `HintStore` of
// @tailor-kit/shell on RequestTailor's storage keys.

import { HintStore as KitHintStore } from "@tailor-kit/shell";

import { shellStorage } from "./storage";

export class HintStore extends KitHintStore {
  constructor() {
    super(shellStorage);
  }
}

/** The one store of the app. */
export const hintStore = new HintStore();

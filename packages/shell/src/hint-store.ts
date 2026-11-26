// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// Which extension hints the user dismissed (wave 14, strand C), remembered per device in
// localStorage. A hint with scope `device` is dismissed once for good; one with scope `target`
// only for the target (collection, request, repository) it was shown for. Storage may be missing, full or hold
// rubbish: then the dismissal lasts until the page is reloaded and nothing breaks.

import { get, writable, type Readable } from "svelte/store";

import type { ShellStorage } from "./storage";

const STORAGE_KEY = "hints.dismissed";
const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === "string");

export class HintStore {
  private readonly set = writable<ReadonlySet<string>>(new Set());

  constructor(private readonly storage: ShellStorage) {
    this.reload();
  }

  /** The dismissed keys; subscribe (`$store`) to react to changes. */
  get dismissed(): Readable<ReadonlySet<string>> {
    return this.set;
  }

  has(key: string): boolean {
    return get(this.set).has(key);
  }

  dismiss(key: string): void {
    const next = new Set(get(this.set));
    next.add(key);
    this.set.set(next);
    this.storage.store(STORAGE_KEY, [...next]);
  }

  /** Reads the device's dismissals again (a fresh page load does this once). */
  reload(): void {
    this.set.set(new Set(this.storage.load<string[]>(STORAGE_KEY, [], isStrings)));
  }
}

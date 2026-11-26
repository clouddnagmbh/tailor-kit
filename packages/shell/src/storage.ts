// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// Per-device conveniences of the shell (theme, panel sizes, last open tabs) in localStorage.
// Storage may be missing or throw (private windows, blocked site data); then nothing is
// remembered and the shell still works. Every key gets the product's prefix (`rt.shell.` in
// RequestTailor), so a product keeps reading what its users stored.

export interface ShellStorage {
  load<T>(key: string, fallback: T, valid: (v: unknown) => v is T): T;
  store(key: string, value: unknown): void;
}

export function createShellStorage(prefix: string): ShellStorage {
  return {
    load(key, fallback, valid) {
      try {
        const raw = globalThis.localStorage?.getItem(prefix + key);
        if (raw == null) return fallback;
        const value: unknown = JSON.parse(raw);
        return valid(value) ? value : fallback;
      } catch {
        return fallback;
      }
    },
    store(key, value) {
      try {
        if (value === undefined) globalThis.localStorage?.removeItem(prefix + key);
        else globalThis.localStorage?.setItem(prefix + key, JSON.stringify(value));
      } catch {
        // Not remembered; nothing else depends on it.
      }
    },
  };
}

export const isNumber = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);

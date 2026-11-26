// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// The file view of a repository (wave 15, strand B) without a DOM: where a path leads, which
// revision a text names, and the letters of the tree.

import type { GitFileState } from "../types";

/** Badge letter of a file state in the tree (`undefined`: unchanged). */
export function stateLetter(state: GitFileState | undefined): "A" | "M" | "D" | "?" | null {
  switch (state) {
    case "modified":
    case "conflicted":
      return "M";
    case "deleted":
      return "D";
    case "untracked":
      return "?";
    default:
      return null;
  }
}

/** A repository path with a leading `/`, no trailing one (`/` stays). */
export function normalizePath(path: string): string {
  const trimmed = path.trim().replace(/\/+/g, "/").replace(/\/$/, "");
  if (trimmed === "") return "/";
  return trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
}

/** The folder of a path, `/` for the top. */
export function parentOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at <= 0 ? "/" : path.slice(0, at);
}

/** The path segments with the path up to each, for a breadcrumb. */
export function crumbs(path: string): { name: string; path: string }[] {
  const parts = path.split("/").filter(Boolean);
  return parts.map((name, i) => ({ name, path: `/${parts.slice(0, i + 1).join("/")}` }));
}

/** What the revision field means: `null` is the working tree. */
export function revOf(text: string): string | null {
  const t = text.trim();
  return t === "" || t.toLowerCase() === "working tree" ? null : t;
}

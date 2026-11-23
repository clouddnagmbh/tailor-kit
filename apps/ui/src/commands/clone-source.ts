// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// What the clone field accepts, and how the UI tells a local repository from a network one.

export const CLONE_PLACEHOLDER = "https://github.com/acme/api-tests.git or a local path";

/** Shown when a browser is asked to clone a repository by its path on this machine. */
export const LOCAL_CLONE_UNSUPPORTED =
  "A browser cannot read folders by path. Clone local repositories in the desktop app, or serve this one over HTTPS.";

/**
 * Whether `text` names a repository on this machine: an absolute or relative path, `~/…`,
 * a Windows drive path or `file://…`. `https://…`, `ssh://…` and `git@host:path` are not.
 */
export function looksLocal(text: string): boolean {
  const t = text.trim();
  if (!t) return false;
  if (/^file:\/\//i.test(t)) return true;
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(t)) return false;
  if (/^[A-Za-z]:[\\/]/.test(t)) return true;
  // scp-like `git@host:path`: a colon before the first slash.
  const colon = t.indexOf(":");
  const slash = t.search(/[/\\]/);
  if (colon > 0 && (slash === -1 || colon < slash)) return false;
  return true;
}

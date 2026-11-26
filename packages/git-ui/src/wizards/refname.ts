// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand G: branch and tag names, checked while typing with the same rules and words as
// `rt_app::git::actions::check_ref_name` (the rules of `git check-ref-format --branch`).

/** `null` when `name` is fine, else why not in plain words. */
export function refNameProblem(name: string): string | null {
  const bad = (why: string) => `"${name}" cannot be used as a name: ${why}.`;
  if (name === "") return bad("it is empty");
  if (name === "HEAD" || name === "@") return bad("Git uses it itself");
  if (name.startsWith("-") || name.startsWith("/") || name.endsWith("/")) {
    return bad('it must not start with "-" or "/" or end with "/"');
  }
  if (name.endsWith(".") || name.endsWith(".lock")) return bad('it must not end with "." or ".lock"');
  if (name.includes("..") || name.includes("//") || name.includes("@{")) {
    return bad('it must not contain "..", "//" or "@{"');
  }
  if (/[\s\x00-\x1f\x7f~^:?*[\\]/.test(name)) return bad("it must not contain spaces or any of ~ ^ : ? * [ \\");
  if (name.split("/").some((part) => part.startsWith("."))) return bad('no part of it may start with "."');
  return null;
}

/** A name suggestion from free text: "Fix login bug" → "fix-login-bug". */
export function suggestRefName(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, "-")
    .replace(/[^a-z0-9./-]/g, "")
    .replace(/-{2,}/g, "-")
    .replace(/\.{2,}/g, ".")
    .replace(/\/{2,}/g, "/")
    .replace(/^[-/.]+|[-/.]+$/g, "")
    .replace(/\.lock$/, "");
}

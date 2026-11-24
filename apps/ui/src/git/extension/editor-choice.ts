// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Which file editor of an extension the user chose last, per kind (`diff`, `merge`), remembered
// per device (`<extension>/<editor>`; empty: the app's own view). The app's own view stays the
// default until the user picks one; a remembered editor that no longer accepts the file or no
// longer exists is not used, the own view shows instead.

import type { ExtensionFileEditor } from "../../extensions";
import type { ExtensionActionRef } from "../../shell/extension-actions";
import { load, store } from "../../shell/storage";

type Kind = ExtensionFileEditor["kind"];
const isString = (v: unknown): v is string => typeof v === "string";
const key = (kind: Kind) => `git.editor.${kind}`;

/** The remembered choice of `kind`, `""` for the app's own view. */
export function chosenEditor(kind: Kind): string {
  return load(key(kind), "", isString);
}

export function chooseEditor(kind: Kind, editor: string): void {
  store(key(kind), editor === "" ? undefined : editor);
}

/** The editor to show: `choice` if some editor of `editors` has that key, else none. */
export function pickEditor<E extends ExtensionFileEditor>(
  editors: readonly ExtensionActionRef<E>[],
  choice: string,
): ExtensionActionRef<E> | null {
  return choice === "" ? null : (editors.find((e) => e.key === choice) ?? null);
}

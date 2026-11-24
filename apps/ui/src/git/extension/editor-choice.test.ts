// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ExtensionFileEditor } from "../../extensions";
import type { ExtensionActionRef } from "../../shell/extension-actions";
import { chooseEditor, chosenEditor, pickEditor } from "./editor-choice";

const ref = (key: string): ExtensionActionRef<ExtensionFileEditor> => ({
  key,
  extension: key.split("/")[0]!,
  action: { id: key.split("/")[1]!, title: key, kind: "diff", component: (() => {}) as never },
});

describe("file editor choice", () => {
  beforeEach(() => {
    const data = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => data.get(k) ?? null,
      setItem: (k: string, v: string) => void data.set(k, v),
      removeItem: (k: string) => void data.delete(k),
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("starts with the app's own view and remembers a choice per kind", () => {
    expect(chosenEditor("diff")).toBe("");
    chooseEditor("diff", "pro/merge");
    expect(chosenEditor("diff")).toBe("pro/merge");
    expect(chosenEditor("merge")).toBe("");
    chooseEditor("diff", "");
    expect(chosenEditor("diff")).toBe("");
  });

  it("shows an editor only if one still has that key", () => {
    const editors = [ref("a/one"), ref("b/two")];
    expect(pickEditor(editors, "")).toBeNull();
    expect(pickEditor(editors, "b/two")?.key).toBe("b/two");
    expect(pickEditor(editors, "gone/one")).toBeNull();
  });
});

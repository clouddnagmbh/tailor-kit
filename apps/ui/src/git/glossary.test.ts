// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { GLOSSARY } from "./glossary";
import { wizardTitle, type GitWizardKind } from "./wizards/open";

describe("Git in plain words", () => {
  it("explains every term without leaning on another Git word", () => {
    for (const [term, gloss] of Object.entries(GLOSSARY)) {
      expect(gloss.label, term).not.toBe("");
      expect(gloss.text.length, term).toBeGreaterThan(20);
      expect(gloss.text, term).not.toMatch(/\b(HEAD|rebase|refspec|index)\b/);
    }
  });

  it("names every wizard in plain words", () => {
    const kinds: GitWizardKind[] = ["create-branch", "merge", "stash", "cherry-pick", "diverged", "detached", "undo"];
    for (const k of kinds) expect(wizardTitle(k)).toMatch(/^[A-Z][a-z]/);
    expect(wizardTitle("stash")).toBe("Park changes");
  });
});

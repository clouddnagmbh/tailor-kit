// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { AI_PROVIDERS, providerInfo } from "./providers";

describe("provider table", () => {
  it("has unique kinds and looks them up (the contract comparison is the product's test)", () => {
    expect(new Set(AI_PROVIDERS.map((p) => p.kind)).size).toBe(AI_PROVIDERS.length);
    expect(providerInfo("anthropic").label).toBe("Anthropic");
    expect(() => providerInfo("nope" as never)).toThrow();
  });
});

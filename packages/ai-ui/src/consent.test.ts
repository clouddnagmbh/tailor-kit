// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { consentLine, formatBytes, previewHost } from "./consent";

describe("consent line", () => {
  it("names destination, model, size and masked secrets", () => {
    expect(consentLine({ endpoint: "https://api.anthropic.com/v1/messages", model: "claude-sonnet", bytes: 316, masked: 1 })).toBe(
      "To api.anthropic.com · claude-sonnet · 316 B · 1 secret masked",
    );
    expect(consentLine({ endpoint: "http://localhost:11434/v1", model: "llama3", bytes: 2048, masked: 0 })).toBe("To localhost:11434 · llama3 · 2.0 KB · no secrets");
    expect(consentLine({ endpoint: "x", model: "m", bytes: 1, masked: 3 })).toContain("3 secrets masked");
  });
  it("formats sizes", () => {
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MB");
    expect(previewHost("https://a.b/c")).toBe("a.b");
  });
});

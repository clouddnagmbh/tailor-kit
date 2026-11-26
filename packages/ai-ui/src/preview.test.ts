// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { segments } from "./preview";

describe("preview text", () => {
  it("marks the places secrets were replaced", () => {
    expect(segments("a *** b")).toEqual([
      { text: "a ", masked: false },
      { text: "***", masked: true },
      { text: " b", masked: false },
    ]);
    expect(segments("plain")).toEqual([{ text: "plain", masked: false }]);
  });
});

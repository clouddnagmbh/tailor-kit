// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import { looksLocal } from "./clone-source";

describe("looksLocal", () => {
  it("recognises paths on this machine", () => {
    for (const t of [
      "/Users/me/origin.git",
      "~/repos/api",
      "../api-tests",
      "api-tests.git",
      "file:///Users/me/origin.git",
      "C:\\repos\\api",
      "  /tmp/x  ",
    ]) {
      expect(looksLocal(t), t).toBe(true);
    }
  });

  it("leaves network addresses alone", () => {
    for (const t of ["https://github.com/acme/api.git", "HTTP://localhost:3000/r.git", "ssh://git@host/r.git", "git@github.com:acme/api.git", ""]) {
      expect(looksLocal(t), t).toBe(false);
    }
  });
});

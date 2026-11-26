// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { inline, parseMarkdown } from "./markdown";

describe("markdown", () => {
  it("parses the subset into text, never HTML", () => {
    expect(inline("a **b** `c` *d* <i>")).toEqual([
      { text: "a " },
      { text: "b", bold: true },
      { text: " " },
      { text: "c", code: true },
      { text: " " },
      { text: "d", italic: true },
      { text: " <i>" },
    ]);
    const blocks = parseMarkdown("# Cause\nThe token\nexpired.\n\n- one\n- two\n\n1. fix\n\n```js\nx()\n```");
    expect(blocks.map((b) => b.kind)).toEqual(["h", "p", "ul", "ol", "code"]);
    expect(blocks[1]).toEqual({ kind: "p", spans: [{ text: "The token expired." }] });
    expect(blocks[4]).toEqual({ kind: "code", text: "x()" });
  });
});

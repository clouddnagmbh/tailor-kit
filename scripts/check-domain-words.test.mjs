// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { findHits, scanTree } from "./check-domain-words.mjs";

const lines = (text, ext) => findHits(text, ext).map((h) => h.line);

test("Rust: code and strings fail, comments and std::collections pass", () => {
  const src = [
    "//! Moved from RequestTailor's collection code.", // 1: inner doc
    "/// A collection of things.", // 2: doc
    "use std::collections::HashMap;", // 3: standard library
    "/* a /* nested collection */ still comment */ let a = 1;", // 4
    'let s = "this collection";', // 5: string
    "let Collection = 1;", // 6: identifier
    "let c = '\"'; let d = \"x\"; // collection", // 7: char literal does not open a string
    'let r = r#"a "collection" b"#;', // 8: raw string
    "fn f<'a>(x: &'a str) {} // collection", // 9: lifetime, then comment
  ].join("\n");
  assert.deepEqual(lines(src, ".rs"), [5, 6, 8]);
});

test("TypeScript and Svelte: comments pass, strings and URLs do not hide a hit", () => {
  const ts = [
    "// collection",
    "/** JSDoc collection */",
    'const u = "https://example.org/collection"; // fine',
    "const t = `a ${x} collection`;",
    "const id = collectionId;",
  ].join("\n");
  assert.deepEqual(lines(ts, ".ts"), [3, 4, 5]);
  const sv = ["<!-- A collection -->", "<p>Open the collection</p>", "<script>", "  // collection", "  const c = 1; /* collection */", "</script>"].join("\n");
  assert.deepEqual(lines(sv, ".svelte"), [2]);
});

test("TOML hash comments pass, values fail; JSON has no comments", () => {
  assert.deepEqual(lines('# a collection\ndescription = "a collection" # collection\n', ".toml"), [2]);
  assert.deepEqual(lines('{ "collection": 1 }', ".json"), [1]);
});

test("Markdown: only HTML comments pass", () => {
  assert.deepEqual(lines("<!-- collection -->\nThe Collection.\n", ".md"), [2]);
});

test("scanTree reads crates/tk-* and packages/*, honours exclusions, skips generated files", () => {
  const root = mkdtempSync(join(tmpdir(), "tk-words-"));
  try {
    const put = (rel, text) => {
      mkdirSync(dirname(join(root, rel)), { recursive: true });
      writeFileSync(join(root, rel), text);
    };
    put("crates/tk-a/src/lib.rs", "let collection = 1;\n");
    put("crates/tk-git/src/lib.rs", "let collection = 1;\n");
    put("crates/rt-app/src/lib.rs", "let collection = 1;\n"); // not the kit
    put("packages/ui/src/a.ts", "export const ok = 1; // collection\n");
    put("packages/ui/src/b.generated.ts", "export const collection = 1;\n");
    put("packages/ui/node_modules/x/index.js", "collection\n");
    const all = scanTree(root).map((h) => h.file);
    assert.deepEqual(all, ["crates/tk-a/src/lib.rs", "crates/tk-git/src/lib.rs"]);
    assert.deepEqual(scanTree(root, ["crates/tk-git"]).map((h) => h.file), ["crates/tk-a/src/lib.rs"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

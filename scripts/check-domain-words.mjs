// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Gate T2 of the tailor-kit extraction: no product words in the shared code.
// The kit (crates/tk-*, packages/*) serves RequestTailor and GitTailor, so it must not
// know what a "collection" is. Provenance comments ("Moved from RequestTailor's
// CollectionDiff ...") are allowed; code, strings, identifiers, JSON and TOML values are not.
//
// Usage: node scripts/check-domain-words.mjs [--summary] [--exclude <prefix>]...
//   --summary   print the hit count per directory, too
//   --exclude   skip paths starting with the prefix (relative to the repo root)
//
// Forbidden word (case-insensitive, anywhere outside a comment): "collection", see WORDS.
// "request" is NOT forbidden: it is the generic HTTP word in tk-net and tk-review.
// "RequestTailor" is not forbidden either: crate descriptions name both consumers and the
// gRPC user agent carries the product name; what would tie the kit to a product is its
// domain model, and that is the word "collection".

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

// "std::collections::" is the Rust standard library, not the product word.
export const WORDS = [/collection(?!s::)/i];
export const ROOTS = ["crates", "packages"];
// Only these crates and packages are the kit: crates/tk-*, packages/*.
const KIT = (rel) => rel.startsWith("packages/") || /^crates\/tk-/.test(rel);
const SOURCE = new Set([".rs", ".ts", ".svelte", ".js", ".mjs", ".css", ".json", ".toml", ".md"]);
const SKIP_DIRS = new Set(["node_modules", "target", "dist", ".svelte-kit", ".git"]);
const SKIP_FILES = [/\.generated\.[a-z]+$/, /(^|\/)package-lock\.json$/, /(^|\/)Cargo\.lock$/];

const KIND = {
  ".rs": "c-rust",
  ".ts": "c",
  ".js": "c",
  ".mjs": "c",
  ".css": "css",
  ".svelte": "svelte",
  ".md": "md",
  ".toml": "hash",
  ".json": "none",
};

/**
 * Replaces every comment character by a space (newlines stay), so line numbers survive.
 * A small state machine, not a parser: good enough to tell code from comments in the
 * languages used here.
 */
export function stripComments(text, ext) {
  const kind = KIND[ext] ?? "none";
  if (kind === "none") return text;
  const out = text.split("");
  const n = text.length;
  const blank = (from, to) => {
    for (let k = from; k < to; k++) if (out[k] !== "\n") out[k] = " ";
  };
  const slashes = kind === "c" || kind === "c-rust" || kind === "css" || kind === "svelte";
  const lineSlashes = slashes && kind !== "css";
  const stringsOpen = kind === "md" ? [] : kind === "hash" ? ['"', "'"] : ['"', "'", "`"];
  let i = 0;
  let depth = 0; // Rust block comments nest
  while (i < n) {
    const c = text[i];
    const two = text.substr(i, 2);
    if (kind === "md" || kind === "svelte") {
      if (text.startsWith("<!--", i)) {
        const end = text.indexOf("-->", i + 4);
        const stop = end < 0 ? n : end + 3;
        blank(i, stop);
        i = stop;
        continue;
      }
    }
    if (kind === "md") {
      i++;
      continue;
    }
    if (kind === "hash") {
      if (c === "#") {
        const stop = text.indexOf("\n", i);
        blank(i, stop < 0 ? n : stop);
        i = stop < 0 ? n : stop;
        continue;
      }
    } else if (slashes && two === "/*") {
      let j = i + 2;
      depth = 1;
      while (j < n && depth > 0) {
        if (kind === "c-rust" && text.startsWith("/*", j)) {
          depth++;
          j += 2;
        } else if (text.startsWith("*/", j)) {
          depth--;
          j += 2;
        } else j++;
      }
      blank(i, j);
      i = j;
      continue;
    } else if (lineSlashes && two === "//") {
      // A URL scheme ("https://") is not a comment start.
      if (!(i > 0 && text[i - 1] === ":" && kind !== "c-rust")) {
        const stop = text.indexOf("\n", i);
        blank(i, stop < 0 ? n : stop);
        i = stop < 0 ? n : stop;
        continue;
      }
    }
    if (stringsOpen.includes(c)) {
      if (kind === "c-rust" && c === "'") {
        // char literal ('a', '\n') or lifetime ('a): only the former has a closing quote
        const m = /^'(\\.[^']*|[^\\'])'/.exec(text.slice(i, i + 12));
        i += m ? m[0].length : 1;
        continue;
      }
      if (kind === "c-rust" && c === '"' && /r#*$/.test(text.slice(Math.max(0, i - 4), i))) {
        const hashes = /(#*)$/.exec(text.slice(Math.max(0, i - 4), i))[1];
        const stop = text.indexOf('"' + hashes, i + 1);
        i = stop < 0 ? n : stop + 1 + hashes.length;
        continue;
      }
      const multiline = c === "`" || kind === "c-rust";
      let j = i + 1;
      while (j < n && text[j] !== c) {
        if (text[j] === "\\") j++;
        else if (text[j] === "\n" && !multiline) break;
        j++;
      }
      i = j + 1;
      continue;
    }
    i++;
  }
  return out.join("");
}

export function findHits(text, ext) {
  const code = stripComments(text, ext).split("\n");
  const lines = text.split("\n");
  const hits = [];
  code.forEach((line, k) => {
    if (WORDS.some((re) => re.test(line))) hits.push({ line: k + 1, text: lines[k].trim() });
  });
  return hits;
}

function walk(dir, root, files) {
  for (const name of readdirSync(dir)) {
    if (SKIP_DIRS.has(name)) continue;
    const path = join(dir, name);
    const st = statSync(path);
    if (st.isDirectory()) walk(path, root, files);
    else {
      const rel = relative(root, path).split(sep).join("/");
      if (SOURCE.has(extname(name)) && KIT(rel) && !SKIP_FILES.some((re) => re.test(rel))) files.push(rel);
    }
  }
}

export function scanTree(root, exclude = []) {
  const files = [];
  for (const r of ROOTS) {
    try {
      walk(join(root, r), root, files);
    } catch {
      // root missing in a partial checkout
    }
  }
  const result = [];
  for (const rel of files.sort()) {
    if (exclude.some((p) => rel.startsWith(p))) continue;
    for (const h of findHits(readFileSync(join(root, rel), "utf8"), extname(rel))) {
      result.push({ file: rel, ...h });
    }
  }
  return result;
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMain) {
  const args = process.argv.slice(2);
  const exclude = [];
  for (let k = 0; k < args.length; k++) if (args[k] === "--exclude") exclude.push(args[++k]);
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const hits = scanTree(root, exclude);
  if (args.includes("--summary")) {
    const per = new Map();
    for (const h of hits) {
      const key = h.file.split("/").slice(0, 2).join("/");
      per.set(key, (per.get(key) ?? 0) + 1);
    }
    for (const [k, v] of [...per].sort()) console.log(`${String(v).padStart(5)}  ${k}`);
  }
  if (hits.length > 0) {
    if (!args.includes("--summary")) {
      console.error("Product words in the kit (outside comments):");
      for (const h of hits) console.error(`  ${h.file}:${h.line}: ${h.text}`);
    }
    console.error(`${hits.length} line(s) with a product word. Provenance in comments is fine; code, UI text and names are not.`);
    process.exit(1);
  }
  console.log("no product words in the kit");
}

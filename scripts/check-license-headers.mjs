// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Fails if a tracked source file lacks the Apache-2.0 SPDX header.
// Usage: node scripts/check-license-headers.mjs

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const HEADER = "SPDX-License-Identifier: Apache-2.0";
const SOURCE = /\.(rs|ts|tsx|js|jsx|mjs|cjs|svelte|vue)$/;
// Canary fixtures deliberately carry foreign licenses.
const SKIP = [/^ci\/canary\//];

const files = execFileSync("git", ["ls-files", "--cached", "--others", "--exclude-standard"], {
  encoding: "utf8",
})
  .split("\n")
  .filter((f) => SOURCE.test(f) && !SKIP.some((re) => re.test(f)));

// The header must sit in the first five lines, not somewhere in the file.
const missing = files.filter((f) => {
  const head = readFileSync(f, "utf8").split("\n", 5).join("\n");
  return !head.includes(HEADER);
});

if (missing.length > 0) {
  console.error(`Missing "${HEADER}" header in the first lines of:`);
  for (const f of missing) console.error(`  ${f}`);
  process.exit(1);
}
console.log(`license headers ok: ${files.length} files`);

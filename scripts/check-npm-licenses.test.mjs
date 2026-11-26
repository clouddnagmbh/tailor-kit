// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { checkLockfile, isAllowed, readAllowlist, readExceptions } from "./check-npm-licenses.mjs";

const allowed = readAllowlist(readFileSync(new URL("../deny.toml", import.meta.url), "utf8"));

test("allowlist is read from deny.toml", () => {
  assert.ok(allowed.has("Apache-2.0"));
  assert.ok(allowed.has("MIT"));
  assert.ok(!allowed.has("GPL-3.0-only"));
});

test("copyleft licenses are rejected", () => {
  for (const id of ["GPL-2.0-only", "GPL-3.0-or-later", "AGPL-3.0-only", "LGPL-2.1-only", "SSPL-1.0"]) {
    assert.equal(isAllowed(id, allowed), false, id);
  }
});

test("SPDX expressions: OR needs one side, AND needs both", () => {
  assert.equal(isAllowed("MIT OR GPL-3.0-only", allowed), true);
  assert.equal(isAllowed("(MIT OR Apache-2.0)", allowed), true);
  assert.equal(isAllowed("MIT AND GPL-3.0-only", allowed), false);
  assert.equal(isAllowed("MIT AND (ISC OR GPL-3.0-only)", allowed), true);
  assert.equal(isAllowed("GPL-3.0-only OR MIT AND AGPL-3.0-only", allowed), false);
  assert.equal(isAllowed("Apache-2.0 WITH LLVM-exception", allowed), true);
  assert.equal(isAllowed("GPL-2.0-only WITH Classpath-exception-2.0", allowed), false);
});

test("malformed expressions throw", () => {
  assert.throws(() => isAllowed("MIT OR", allowed));
  assert.throws(() => isAllowed("(MIT", allowed));
  assert.throws(() => isAllowed("MIT Apache-2.0", allowed));
});

test("GPL canary lockfile is rejected", () => {
  const lock = JSON.parse(
    readFileSync(new URL("../ci/canary/npm-gpl/package-lock.json", import.meta.url), "utf8"),
  );
  const violations = checkLockfile(lock, allowed);
  assert.deepEqual(violations.map((v) => v.name), ["fake-gpl-test-dep"]);
});

test("missing license is a violation", () => {
  const lock = { packages: { "node_modules/x": { version: "1.0.0" } } };
  assert.equal(checkLockfile(lock, allowed)[0].reason, "no license declared");
});

const exceptions = readExceptions(
  readFileSync(new URL("./npm-license-exceptions.json", import.meta.url), "utf8"),
);

test("repository lockfile passes", () => {
  const lock = JSON.parse(readFileSync(new URL("../package-lock.json", import.meta.url), "utf8"));
  assert.deepEqual(checkLockfile(lock, allowed, exceptions), []);
});

test("a dev-only exception does not cover a shipped dependency", () => {
  const lock = {
    packages: {
      "node_modules/lightningcss": { license: "MPL-2.0", dev: true },
      "node_modules/lightningcss-darwin-arm64": { license: "MPL-2.0" },
    },
  };
  const v = checkLockfile(lock, allowed, exceptions);
  assert.deepEqual(v.map((x) => x.name), ["lightningcss-darwin-arm64"]);
  assert.match(v[0].reason, /dev dependencies only/);
});

test("an exception covers only its own license", () => {
  const lock = { packages: { "node_modules/lightningcss": { license: "GPL-3.0-only", dev: true } } };
  assert.equal(checkLockfile(lock, allowed, exceptions).length, 1);
});

test("the GPL canary is still rejected with exceptions loaded", () => {
  const lock = JSON.parse(
    readFileSync(new URL("../ci/canary/npm-gpl/package-lock.json", import.meta.url), "utf8"),
  );
  assert.equal(checkLockfile(lock, allowed, exceptions).length, 1);
});

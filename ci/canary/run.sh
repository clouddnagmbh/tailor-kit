#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The RequestTailor Authors
#
# Proves the license gates bite: each canary contains a GPL dependency and MUST be rejected.
# Exits 0 only if both gates fail on their canary for the right reason.
set -uo pipefail
cd "$(dirname "$0")/../.."

fail=0

out=$(cargo deny --manifest-path ci/canary/cargo-gpl/Cargo.toml --config deny.toml check licenses 2>&1)
if [[ $? -ne 0 && "$out" == *"GPL-3.0-only"* ]]; then
  echo "ok: cargo-deny rejects the GPL canary"
else
  echo "FAIL: cargo-deny did not reject the GPL canary"; echo "$out"; fail=1
fi

out=$(node scripts/check-npm-licenses.mjs ci/canary/npm-gpl/package-lock.json deny.toml 2>&1)
if [[ $? -ne 0 && "$out" == *"fake-gpl-test-dep: GPL-3.0-only"* ]]; then
  echo "ok: npm license check rejects the GPL canary"
else
  echo "FAIL: npm license check did not reject the GPL canary"; echo "$out"; fail=1
fi

exit $fail

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  test: { include: ["src/**/*.test.ts"] },
});

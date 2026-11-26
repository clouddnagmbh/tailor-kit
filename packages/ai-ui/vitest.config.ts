// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  test: {
    include: ["src/**/*.test.ts"],
    // @tailor-kit/ui ships .svelte sources (and Lucide icons); compile them instead of loading them as Node modules.
    server: { deps: { inline: [/@lucide\/svelte/, /@tailor-kit\/ui/] } },
  },
});

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it, vi } from "vitest";

import type { GitHost } from "../host";
import { openNewPull } from "./open-new-pull";

describe("openNewPull", () => {
  it("opens the dialog without a prefill by default", async () => {
    const showDialog = vi.fn(async () => undefined);
    await openNewPull({ showDialog } as unknown as GitHost);
    const arg = showDialog.mock.calls[0] as unknown as [{ props: unknown; title: string }];
    expect(arg[0].props).toEqual({});
    expect(arg[0].title).toBe("Create pull request");
  });

  it("hands title, body, base and head to the dialog", async () => {
    const showDialog = vi.fn(async () => undefined);
    const prefill = { title: "Add currency", body: "Why\n\nWhat", base: "main", head: "feature/x" };
    await openNewPull({ showDialog } as unknown as GitHost, prefill);
    const arg = showDialog.mock.calls[0] as unknown as [{ props: unknown }];
    expect(arg[0].props).toEqual(prefill);
  });
});

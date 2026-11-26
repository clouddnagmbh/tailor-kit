// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

import { describe, expect, it } from "vitest";

import { createAiClient, type AiClientApi } from "./extension-ai";
import type { AiPolicy, AiPreview, AiSettings } from "./types";

class Failure extends Error {
  constructor(
    readonly kind: string,
    message: string,
  ) {
    super(message);
  }
}

function setup(policy: AiPolicy = { file_off: false, device_off: false }, required = true) {
  let settings: AiSettings | null = {
    profiles: [{ id: "p", label: "P", provider: "anthropic", model: "m", base_url: null }],
    roles: {},
    preview: "first-use",
  };
  const log: string[] = [];
  const api: AiClientApi<{ s: number }, string> = {
    settings: async () => settings,
    setSettings: async (s) => void (settings = s),
    policy: async () => policy,
    preview: async (what, input) => {
      log.push(`preview:${input}:${JSON.stringify(what.request.scope)}`);
      return { profile: "p", required, call: { system: "", messages: [], max_tokens: 1 } } as unknown as AiPreview;
    },
    confirmPreview: async (p) => void log.push(`confirm:${p}`),
    complete: async (_r, input) => {
      log.push(`complete:${input}`);
      return { text: "ok", stop: "end" };
    },
    cancel: async () => undefined,
  };
  const client = (confirm: (p: AiPreview) => Promise<boolean>) =>
    createAiClient({ api, input: () => "in", confirm, emptyScope: { s: 0 }, fail: (k, m) => new Failure(k, m), optedOutMessage: "off" });
  return { log, client };
}

const request = { purpose: "x", system: "s", messages: [] };

describe("createAiClient", () => {
  it("previews, remembers the confirmation, then sends with the host's input and empty scope", async () => {
    const { log, client } = setup();
    const reply = await client(async () => true).complete(request);
    expect(reply.text).toBe("ok");
    expect(log).toEqual(['preview:in:{"s":0}', "confirm:p", "complete:in"]);
  });

  it("refuses an opted-out scope before building a call, with the host's error", async () => {
    const { log, client } = setup({ file_off: true, device_off: false });
    await expect(client(async () => true).complete(request)).rejects.toMatchObject({ kind: "opted-out", message: "off" });
    expect(log).toEqual([]);
  });

  it("sends nothing when the preview is declined", async () => {
    const { log, client } = setup();
    await expect(client(async () => false).complete(request)).rejects.toMatchObject({ kind: "cancelled" });
    expect(log).toEqual(['preview:in:{"s":0}']);
  });

  it("reports status with the provider's capabilities", async () => {
    const { client } = setup();
    const status = await client(async () => true).status();
    expect(status).toMatchObject({ configured: true, allowed: true, fast: "p", smart: "p" });
    expect(status.profiles[0]).toMatchObject({ tools: true, streaming: true });
  });
});

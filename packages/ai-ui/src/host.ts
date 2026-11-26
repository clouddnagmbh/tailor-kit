// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// What the AI settings dialog needs from the product around it, and nothing else (like `GitHost`
// of the Git views): the AI part of the backend, the brand texts, the current settings and two
// actions. The dialog gets it with `getAiHost()`; the product puts it there once with
// `setAiHost()` above the dialog. Keys go through `backend.ai`; the UI never stores one.

import { getContext, setContext } from "svelte";

import type { AiPolicy, AiProviderKind, AiSettings } from "./types";

/** The parts of the product's wording the AI dialogs print. */
export interface AiBrand {
  /** "RequestTailor", "GitTailor": names the product in the settings dialog. */
  productName: string;
  /** The command of the local companion the web app needs for providers that refuse browser
   *  calls (`rt agent`); `null` for a product without one. */
  companionCommand: string | null;
  /** What the on/off switch of the policy is about: "collection", "repository". */
  scopeNoun: string;
  /** Shown instead of the switch's hint when the team turned the assistant off in a file. */
  fileOffNote: string;
  /** The "what is sent" paragraph at the end of the dialog (what the product puts into a call). */
  sentText: string;
}

/** The calls of `Backend.ai` the settings dialog makes (a subset of the product's `AiApi`). */
export interface AiSettingsApi {
  settings(): Promise<AiSettings | null>;
  setSettings(settings: AiSettings | null): Promise<void>;
  hasKey(provider: AiProviderKind): Promise<boolean>;
  deleteKey(provider: AiProviderKind): Promise<void>;
  hasProfileKey(profile: string): Promise<boolean>;
  setProfileKey(profile: string, key: string): Promise<void>;
  deleteProfileKey(profile: string): Promise<void>;
  policy(): Promise<AiPolicy>;
  setDeviceOptOut(off: boolean): Promise<void>;
}

export interface AiHost {
  backend: {
    /** `"web"`: keys live in the encrypted browser store and some providers need the companion. */
    kind: string;
    ai: AiSettingsApi;
  };
  brand: AiBrand;
  /** The settings as last read; `null` until set up. Reactive when the product's state is. */
  readonly settings: AiSettings | null;
  /** Reads the settings (and what depends on them) again after the dialog changed them. */
  reload(): Promise<void>;
  /** Closes the dialog. */
  close(): void;
}

const KEY = Symbol("tailor-kit.ai-host");

export function setAiHost(host: AiHost): void {
  setContext(KEY, host);
}

export function getAiHost(): AiHost {
  const host = getContext<AiHost | undefined>(KEY);
  if (!host) throw new Error("AI host missing: call setAiHost() above the AI dialogs");
  return host;
}

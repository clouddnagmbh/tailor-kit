// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// Wave 16 interface: the AI provider for extensions, over the product's `Backend.ai`, here
// without the product: the backend calls, the input of a send and the error class are passed in.
// The flow is fixed here; the backend does the masking and the call.
//
// 1. `policy()`: an opted-out scope refuses before anything is built.
// 2. `preview()`: the exact, masked call. If the settings require it (first use of a profile,
//    or always) or the caller asks, the user sees it and confirms; a first-use confirmation is
//    remembered (`confirmPreview`).
// 3. `complete()`: streamed pieces go to `onDelta`; aborting the signal cancels the call.

import type { Component } from "svelte";

import PreviewDialog from "./PreviewDialog.svelte";
import { providerInfo } from "./providers";
import type { AiDelta, AiEvent, AiPolicy, AiPreview, AiProviderKind, AiReply, AiRequest, AiSettings } from "./types";

/** A configured profile as an extension sees it (never a key). */
export interface ExtensionAiProfile {
  id: string;
  label: string;
  provider: AiProviderKind;
  model: string;
  /** What the provider can do (`tk_ai::AiProviderKind::capabilities`). */
  tools: boolean;
  streaming: boolean;
}

export interface ExtensionAiStatus {
  /** At least one profile is set up. */
  configured: boolean;
  /** The active scope allows the assistant (a file of the repository and this device). */
  allowed: boolean;
  profiles: ExtensionAiProfile[];
  /** The profiles the roles `fast` and `smart` resolve to, if any. */
  fast: string | null;
  smart: string | null;
}

/** A call: the extension's own prompt; the core adds the masked context of `scope` (`S`, the
 *  product's own type). */
export type ExtensionAiRequest<S = unknown> = Omit<AiRequest<S>, "id" | "scope"> & { scope?: S };

export interface ExtensionAiOptions {
  /** Aborting cancels the call (the promise rejects with kind `cancelled`). */
  signal?: AbortSignal;
  /** Streamed pieces of the answer as they arrive. */
  onDelta?: (delta: AiDelta) => void;
  /** Show the "what will be sent" preview even when the settings do not require it. */
  preview?: boolean;
}

/** `ExtensionContext.ai`. Calls go only to the user's own provider through the app's transport;
 *  secret values in scope are masked before anything leaves. Errors carry the kinds of `AiError`
 *  (`not-configured`, `no-key`, `opted-out`, `cancelled`, …). */
export interface ExtensionAi<S = unknown> {
  status(): Promise<ExtensionAiStatus>;
  complete(request: ExtensionAiRequest<S>, options?: ExtensionAiOptions): Promise<AiReply>;
}

/** The calls of `Backend.ai` the client makes; `I` is the product's input of a send. */
export interface AiClientApi<S, I> {
  settings(): Promise<AiSettings | null>;
  setSettings(settings: AiSettings | null): Promise<void>;
  policy(): Promise<AiPolicy>;
  preview(what: { kind: "request"; request: AiRequest<S> }, input: I): Promise<AiPreview>;
  confirmPreview(profile: string): Promise<void>;
  complete(request: AiRequest<S>, input: I, onEvent?: (event: AiEvent) => void): Promise<AiReply>;
  cancel(id: string): Promise<void>;
}

/** Asks the user to confirm a preview; `true` sends. */
export type ConfirmPreview = (preview: AiPreview) => Promise<boolean>;

/** A dialog the product's shell shows (`ExtensionDialog`, `GitDialog`). */
export interface AiDialogSpec {
  title: string;
  component: Component<{ close: (result?: unknown) => void; props: Record<string, unknown> }>;
  props: Record<string, unknown>;
  width: number;
}

let seq = 0;

/** A call id unique in this page. */
export function nextCallId(): string {
  seq += 1;
  return `ext-${Date.now().toString(36)}-${seq}`;
}

/** The preview in the shell's dialog place. */
export function dialogConfirm(
  showDialog: (dialog: AiDialogSpec) => Promise<unknown>,
  api: Pick<AiClientApi<unknown, unknown>, "settings" | "setSettings">,
): ConfirmPreview {
  return async (preview) => {
    const answer = (await showDialog({
      title: "What will be sent",
      component: PreviewDialog,
      props: { preview },
      width: 720,
    })) as { send?: boolean; remember?: boolean } | undefined;
    if (answer?.send !== true) return false;
    if (answer.remember) {
      // "Don't show this automatically": previews only on request from now on.
      const settings = await api.settings();
      if (settings) await api.setSettings({ ...settings, preview: "on-request" });
    }
    return true;
  };
}

export interface AiClientOptions<S, I> {
  api: AiClientApi<S, I>;
  /** The input of a send (RequestTailor: the active environment). */
  input: () => I;
  confirm: ConfirmPreview;
  /** The scope of a request that names none. */
  emptyScope: S;
  /** The error the product's backend rejects with (`BackendError`). */
  fail: (kind: string, message: string) => Error;
  /** The message of the `opted-out` refusal. */
  optedOutMessage: string;
}

export function createAiClient<S, I>(o: AiClientOptions<S, I>): ExtensionAi<S> {
  const { api } = o;
  return {
    async status(): Promise<ExtensionAiStatus> {
      const [settings, policy] = await Promise.all([api.settings(), api.policy()]);
      const profiles = (settings?.profiles ?? []).map((p) => {
        const caps = providerInfo(p.provider).capabilities;
        return { id: p.id, label: p.label, provider: p.provider, model: p.model, tools: caps.tools, streaming: caps.streaming };
      });
      const known = (id: string | undefined) => (id && profiles.some((p) => p.id === id) ? id : null);
      const fallback = known(settings?.default_profile) ?? profiles[0]?.id ?? null;
      return {
        configured: profiles.length > 0,
        allowed: policy.file_off === false && policy.device_off === false,
        profiles,
        fast: known(settings?.roles.fast) ?? fallback,
        smart: known(settings?.roles.smart) ?? fallback,
      };
    },

    async complete(request: ExtensionAiRequest<S>, options: ExtensionAiOptions = {}): Promise<AiReply> {
      const policy = await api.policy();
      if (policy.file_off || policy.device_off) throw o.fail("opted-out", o.optedOutMessage);
      const full: AiRequest<S> = { ...request, id: nextCallId(), scope: request.scope ?? o.emptyScope };
      const preview = await api.preview({ kind: "request", request: full }, o.input());
      if (preview.required || options.preview) {
        if (!(await o.confirm(preview))) throw o.fail("cancelled", "cancelled");
        if (preview.required) await api.confirmPreview(preview.profile);
      }
      if (options.signal?.aborted) throw o.fail("cancelled", "cancelled");
      const abort = () => void api.cancel(full.id);
      options.signal?.addEventListener("abort", abort, { once: true });
      try {
        return await api.complete(full, o.input(), (event) => {
          if (event.kind === "delta") options.onDelta?.(event.delta);
        });
      } finally {
        options.signal?.removeEventListener("abort", abort);
      }
    },
  };
}

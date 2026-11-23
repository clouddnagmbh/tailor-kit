<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- AI assistant settings (wave 8, strand E): provider, model, base URL and key, and what is
     sent. No provider is chosen in advance; the assistant stays off until one is saved. The
     key goes to the keychain (desktop) or the encrypted browser store (web) and never comes
     back: the dialog only learns whether one is set. -->
<script lang="ts">
  import type { AiProviderKind } from "../commands/types";
  import type { AiState } from "./ai.svelte";
  import AiDialog from "./AiDialog.svelte";

  let { ai }: { ai: AiState } = $props();

  // Wave 16: the other providers get their dialog with profiles (strand A).
  const DEFAULT_MODEL: Record<AiProviderKind, string> = {
    anthropic: "claude-opus-5-5",
    "openai-compatible": "",
    openai: "",
    "azure-openai": "",
    gemini: "",
    mistral: "",
    bedrock: "",
  };

  // The dialog edits a copy; nothing changes until "Save".
  // svelte-ignore state_referenced_locally
  let provider = $state<AiProviderKind | "">(ai.config?.provider ?? "");
  // svelte-ignore state_referenced_locally
  let model = $state(ai.config?.model ?? "");
  // svelte-ignore state_referenced_locally
  let baseUrl = $state(ai.config?.base_url ?? "");
  let key = $state("");
  // svelte-ignore state_referenced_locally
  let keySet = $state(ai.config ? ai.hasKey : false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function choose(next: AiProviderKind | "") {
    provider = next;
    error = null;
    if (next === "") return;
    if (model.trim() === "" || model === DEFAULT_MODEL.anthropic) model = DEFAULT_MODEL[next];
    keySet = await ai.backend.ai.hasKey(next).catch(() => false);
  }

  async function save() {
    if (provider === "") return;
    busy = true;
    error = null;
    try {
      const config = { provider, model: model.trim(), base_url: baseUrl.trim() === "" ? null : baseUrl.trim() };
      await ai.backend.ai.setConfig(config);
      if (key.trim() !== "") await ai.backend.ai.setKey(provider, key);
      key = "";
      await ai.reload();
      ai.close();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function removeKey() {
    if (provider === "") return;
    busy = true;
    try {
      await ai.backend.ai.deleteKey(provider);
      keySet = false;
      await ai.reload();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function turnOff() {
    busy = true;
    try {
      await ai.backend.ai.setConfig(null);
      await ai.reload();
      ai.close();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  const needsKey = $derived(provider === "anthropic");
  const canSave = $derived(
    provider !== "" &&
      model.trim() !== "" &&
      (provider !== "openai-compatible" || baseUrl.trim() !== "") &&
      (!needsKey || keySet || key.trim() !== ""),
  );
</script>

<AiDialog title="AI assistant" testid="ai-settings" {busy} onclose={() => ai.close()}>
  <p>
    The assistant is off until you choose a provider. Use your own API key or a model running on your computer;
    RequestTailor has no AI service of its own.
  </p>
  <label class="field"
    >Provider
    <select data-testid="ai-provider" value={provider} onchange={(e) => choose(e.currentTarget.value as AiProviderKind | "")}>
      <option value="">Choose a provider…</option>
      <option value="anthropic">Anthropic (Claude API)</option>
      <option value="openai-compatible">OpenAI-compatible endpoint (e.g. Ollama, LM Studio)</option>
    </select>
  </label>
  {#if provider !== ""}
    <label class="field"
      >Model
      <input
        data-testid="ai-model"
        bind:value={model}
        placeholder={provider === "anthropic" ? "claude-opus-5-5" : "llama3.1"}
        autocomplete="off"
        spellcheck="false"
      />
    </label>
    <label class="field"
      >Base URL{provider === "anthropic" ? " (optional)" : ""}
      <input
        data-testid="ai-base-url"
        bind:value={baseUrl}
        placeholder={provider === "anthropic" ? "https://api.anthropic.com" : "http://localhost:11434/v1"}
        autocomplete="off"
        spellcheck="false"
      />
    </label>
    <label class="field"
      >API key{needsKey ? "" : " (optional)"}
      <input
        data-testid="ai-key"
        type="password"
        bind:value={key}
        placeholder={keySet ? "A key is set; type to replace it" : needsKey ? "Your API key" : "Only if the endpoint needs one"}
        autocomplete="off"
      />
    </label>
    {#if keySet}
      <p class="muted" data-testid="ai-key-set">
        A key is stored {ai.backend.kind === "web" ? "encrypted in this browser" : "in the system keychain"}.
        <button data-testid="ai-remove-key" disabled={busy} onclick={removeKey}>Remove key</button>
      </p>
    {/if}
  {/if}
  <p class="muted" data-testid="ai-privacy">
    What is sent: only when you ask, the task, the request (method, URL, headers, body, scripts) and for explain and tests
    the response, to the provider above and nowhere else. Secret values are replaced by *** before anything leaves this
    app, and so are credentials typed into headers, URLs or bodies. Proposals are shown to you first and never saved by
    the assistant.
  </p>
  {#if error}<p class="error" role="alert" data-testid="ai-settings-error">{error}</p>{/if}
  <div class="row">
    <button class="primary" data-testid="ai-save" disabled={busy || !canSave} onclick={save}>Save</button>
    <button data-testid="ai-cancel" disabled={busy} onclick={() => ai.close()}>Cancel</button>
    {#if ai.config}
      <button data-testid="ai-turn-off" disabled={busy} onclick={turnOff}>Turn off</button>
    {/if}
  </div>
</AiDialog>

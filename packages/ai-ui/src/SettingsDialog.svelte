<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- AI assistant settings (wave 8; profiles in wave 16): a list of profiles (one model behind
     one provider each), which profile does the quick and which the demanding tasks, when the
     "what will be sent" preview shows, and whether the assistant may be used in this
     scope (collection, repository). No provider is chosen in advance; the assistant stays off until one is saved.
     Keys go to the keychain (desktop) or the encrypted browser store (web) and never come back:
     the dialog only learns whether one is set. -->
<script lang="ts">
  import { untrack } from "svelte";

  import { Badge, Button, Dialog, DialogFooter, Icon, IconButton, SegmentedControl, Select, Switch, TextField } from "@tailor-kit/ui";

  import { getAiHost } from "./host";
  import { AI_PROVIDERS, providerInfo } from "./providers";
  import type { AiPreviewMode, AiProviderKind, AiSettings } from "./types";

  const ai = getAiHost();
  const brand = ai.brand;

  interface Draft {
    id: string;
    label: string;
    provider: AiProviderKind | "";
    model: string;
    baseUrl: string;
    /** A key typed in this dialog (never filled in from storage). */
    key: string;
    keySet: boolean;
    /** Saved before (its key slots can be asked about). */
    saved: boolean;
  }

  const PLACEHOLDER_MODEL: Record<AiProviderKind, string> = {
    anthropic: "claude-opus-5-5",
    openai: "gpt-5",
    "azure-openai": "your deployment name",
    gemini: "gemini-3-pro",
    mistral: "mistral-large-latest",
    "openai-compatible": "llama3.1",
    bedrock: "model id",
  };
  const PLACEHOLDER_URL: Record<AiProviderKind, string> = {
    anthropic: "https://api.anthropic.com",
    openai: "https://api.openai.com/v1",
    "azure-openai": "https://<resource>.openai.azure.com/openai/v1",
    gemini: "https://generativelanguage.googleapis.com",
    mistral: "https://api.mistral.ai/v1",
    "openai-compatible": "http://localhost:11434/v1",
    bedrock: "https://bedrock-runtime.<region>.amazonaws.com",
  };

  const existing = untrack(() => ai.settings);
  let drafts = $state<Draft[]>(
    (existing?.profiles ?? []).map((p) => ({
      id: p.id,
      label: p.label,
      provider: p.provider,
      model: p.model,
      baseUrl: p.base_url ?? "",
      key: "",
      keySet: false,
      saved: true,
    })),
  );
  let selected = $state(0);
  let defaultId = $state(untrack(() => existing?.default_profile ?? ""));
  let fast = $state(untrack(() => existing?.roles.fast ?? ""));
  let smart = $state(untrack(() => existing?.roles.smart ?? ""));
  let preview = $state<AiPreviewMode>(untrack(() => existing?.preview ?? "first-use"));
  let removed = $state<string[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let allowed = $state(true);
  let fileOff = $state(false);
  let hasScope = $state(false);

  function blank(): Draft {
    const taken = new Set(drafts.map((d) => d.id));
    let n = taken.size === 0 ? 0 : 2;
    let id = n === 0 ? "default" : `profile-${n}`;
    while (taken.has(id)) id = `profile-${++n}`;
    return { id, label: "", provider: "", model: "", baseUrl: "", key: "", keySet: false, saved: false };
  }

  if (drafts.length === 0) drafts.push(blank());

  // Whether keys are set, and the policy's switch: asked once, on open.
  void (async () => {
    for (const d of drafts) {
      if (!d.saved) continue;
      const own = await ai.backend.ai.hasProfileKey(d.id).catch(() => false);
      d.keySet = own || (d.provider !== "" && (await ai.backend.ai.hasKey(d.provider).catch(() => false)));
    }
    try {
      const p = await ai.backend.ai.policy();
      fileOff = p.file_off;
      allowed = !p.device_off;
      hasScope = true;
    } catch {
      hasScope = false;
    }
  })();

  const current = $derived(drafts[selected] ?? drafts[0]!);
  const info = $derived(current.provider === "" ? null : providerInfo(current.provider));
  const needsKey = $derived(current.provider !== "" && current.provider !== "openai-compatible");
  const providerOptions = $derived([
    { value: "", label: "Choose a provider…" },
    ...AI_PROVIDERS.map((p) => ({ value: p.kind, label: p.label })),
  ]);
  const complete = (d: Draft) =>
    d.provider !== "" &&
    d.model.trim() !== "" &&
    (!providerInfo(d.provider).capabilities.needs_base_url || d.baseUrl.trim() !== "") &&
    (d.provider === "openai-compatible" || d.keySet || d.key.trim() !== "");
  const ready = $derived(drafts.filter((d) => d.provider !== ""));
  const canSave = $derived(ready.length > 0 && ready.every(complete));
  const nameOf = (d: Draft) => d.label.trim() || (d.provider === "" ? "New profile" : providerInfo(d.provider).label);

  function choose(provider: string) {
    const d = current;
    d.provider = provider as AiProviderKind | "";
    error = null;
    if (provider === "") return;
    if (d.model.trim() === "" || d.model === PLACEHOLDER_MODEL.anthropic) {
      d.model = provider === "anthropic" ? PLACEHOLDER_MODEL.anthropic : "";
    }
    if (d.label.trim() === "") d.label = providerInfo(d.provider as AiProviderKind).label;
  }

  function addProfile() {
    drafts.push(blank());
    selected = drafts.length - 1;
  }

  function removeProfile(i: number) {
    const [gone] = drafts.splice(i, 1);
    if (gone?.saved) removed.push(gone.id);
    if (drafts.length === 0) drafts.push(blank());
    selected = Math.min(selected, drafts.length - 1);
    if (defaultId === gone?.id) defaultId = "";
    if (fast === gone?.id) fast = "";
    if (smart === gone?.id) smart = "";
  }

  async function save() {
    busy = true;
    error = null;
    try {
      const profiles = ready.map((d) => ({
        id: d.id,
        label: nameOf(d),
        provider: d.provider as AiProviderKind,
        model: d.model.trim(),
        base_url: d.baseUrl.trim() === "" ? null : d.baseUrl.trim(),
      }));
      const ids = new Set(profiles.map((p) => p.id));
      const settings: AiSettings = {
        profiles,
        roles: { ...(ids.has(fast) ? { fast } : {}), ...(ids.has(smart) ? { smart } : {}) },
        preview,
        ...(ids.has(defaultId) ? { default_profile: defaultId } : {}),
        previewed: existing?.previewed,
      };
      await ai.backend.ai.setSettings(settings);
      for (const d of ready) if (d.key.trim() !== "") await ai.backend.ai.setProfileKey(d.id, d.key);
      for (const id of removed) await ai.backend.ai.deleteProfileKey(id).catch(() => undefined);
      await ai.reload();
      ai.close();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function removeKey() {
    const d = current;
    busy = true;
    try {
      await ai.backend.ai.deleteProfileKey(d.id);
      if (d.provider !== "") await ai.backend.ai.deleteKey(d.provider);
      d.keySet = false;
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
      await ai.backend.ai.setSettings(null);
      await ai.reload();
      ai.close();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function allow(on: boolean) {
    allowed = on;
    try {
      await ai.backend.ai.setDeviceOptOut(!on);
    } catch (e) {
      allowed = !on;
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const profileOptions = $derived(ready.map((d) => ({ value: d.id, label: nameOf(d) })));
  const PREVIEW_OPTIONS = [
    { value: "first-use", label: "First time" },
    { value: "always", label: "Every time" },
    { value: "on-request", label: "Only when I ask" },
  ];
</script>

<Dialog
  title="AI assistant"
  icon="spark"
  size="lg"
  testid="ai-settings"
  onclose={() => !busy && ai.close()}
  description="Use your own key or a model on your computer. {brand.productName} has no AI service of its own."
>
  <div class="stack">
    <section aria-label="Profiles">
      <div class="head">
        <h3>Profiles</h3>
        <Button size="sm" variant="ghost" icon="add" data-testid="ai-add-profile" onclick={addProfile}>Add profile</Button>
      </div>
      <p class="muted">A profile is one model with its key. A small one for quick answers and a large one for scripts and fixes work well together.</p>
      {#if drafts.length > 1 || current.provider !== ""}
        <ul class="profiles" data-testid="ai-profiles">
          {#each drafts as d, i (d.id)}
            <li>
              <button type="button" class="profile" class:on={i === selected} data-testid="ai-profile" onclick={() => (selected = i)}>
                <span class="name">{nameOf(d)}</span>
                <span class="muted">{d.provider === "" ? "not set up" : `${providerInfo(d.provider).label} · ${d.model || "no model"}`}</span>
                {#if d.provider !== "" && (d.keySet || d.key !== "")}<Badge kind="label" value="key set" tone="ok" />{/if}
              </button>
              {#if drafts.length > 1}
                <IconButton icon="delete" label="Remove profile" size="sm" onclick={() => removeProfile(i)} />
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <div class="form">
        <Select label="Provider" data-testid="ai-provider" value={current.provider} options={providerOptions} onchange={(e) => choose(e.currentTarget.value)} />
        {#if current.provider !== ""}
          <TextField label="Name" data-testid="ai-name" bind:value={current.label} placeholder={info?.label} autocomplete="off" />
          <TextField
            label={current.provider === "azure-openai" ? "Deployment name" : "Model"}
            data-testid="ai-model"
            bind:value={current.model}
            placeholder={PLACEHOLDER_MODEL[current.provider]}
            autocomplete="off"
            spellcheck="false"
          />
          <TextField
            label={info?.capabilities.needs_base_url ? "Base URL" : "Base URL (optional)"}
            data-testid="ai-base-url"
            bind:value={current.baseUrl}
            placeholder={PLACEHOLDER_URL[current.provider]}
            autocomplete="off"
            spellcheck="false"
          />
          <TextField
            label={needsKey ? "API key" : "API key (optional)"}
            data-testid="ai-key"
            type="password"
            bind:value={current.key}
            placeholder={current.keySet ? "A key is set; type to replace it" : needsKey ? "Your API key" : "Only if the endpoint needs one"}
            autocomplete="off"
          />
          {#if current.keySet}
            <p class="muted" data-testid="ai-key-set">
              A key is stored {ai.backend.kind === "web" ? "encrypted in this browser" : "in the system keychain"}.
              <Button size="sm" variant="ghost" data-testid="ai-remove-key" disabled={busy} onclick={removeKey}>Remove key</Button>
            </p>
          {/if}
          {#if current.provider === "openai-compatible"}
            <p class="muted" data-testid="ai-local-note">Models on your own computer keep your data there. Tool use depends on the model.</p>
          {:else if !info?.capabilities.browser_direct && ai.backend.kind === "web"}
            <p class="muted" data-testid="ai-companion-note">
              {info?.label} does not allow calls from a web page. In the web app this needs the {brand.productName} companion{#if brand.companionCommand} (<code>{brand.companionCommand}</code>){/if}.
            </p>
          {/if}
        {/if}
      </div>
    </section>

    {#if ready.length > 1}
      <section aria-label="Which profile does what" data-testid="ai-roles">
        <h3>Which profile does what</h3>
        <div class="grid">
          <Select label="Default" data-testid="ai-default" bind:value={defaultId} options={[{ value: "", label: "The first profile" }, ...profileOptions]} />
          <Select label="Quick tasks (names, short explanations)" data-testid="ai-role-fast" bind:value={fast} options={[{ value: "", label: "The default" }, ...profileOptions]} />
          <Select label="Demanding tasks (scripts, fixes)" data-testid="ai-role-smart" bind:value={smart} options={[{ value: "", label: "The default" }, ...profileOptions]} />
        </div>
      </section>
    {/if}

    <section aria-label="What will be sent">
      <h3>Show what will be sent</h3>
      <SegmentedControl
        label="When to show the preview"
        bind:value={preview}
        options={PREVIEW_OPTIONS}
        data-testid="ai-preview-mode"
      />
      <p class="muted">The preview is always shown the first time a profile is used. It lists everything that leaves this app.</p>
    </section>

    {#if hasScope}
      <section aria-label={`This ${brand.scopeNoun}`}>
        <h3>This {brand.scopeNoun}</h3>
        <Switch
          label={`Allow the AI assistant in this ${brand.scopeNoun}`}
          description={fileOff
            ? brand.fileOffNote
            : `Turn it off here if this ${brand.scopeNoun} must never go to an AI provider. This device only.`}
          checked={allowed && !fileOff}
          disabled={fileOff || busy}
          data-testid="ai-allow"
          onchange={(e) => allow(e.currentTarget.checked)}
        />
      </section>
    {/if}

    <p class="muted" data-testid="ai-privacy">
      {brand.sentText}
    </p>
    {#if error}<p class="error" role="alert" data-testid="ai-settings-error"><Icon name="error" size="sm" /> {error}</p>{/if}
  </div>

  <DialogFooter>
    {#snippet start()}
      {#if existing}
        <Button variant="ghost" data-testid="ai-turn-off" disabled={busy} onclick={turnOff}>Turn off</Button>
      {/if}
    {/snippet}
    <Button data-testid="ai-cancel" disabled={busy} onclick={() => ai.close()}>Cancel</Button>
    <Button variant="primary" data-testid="ai-save" disabled={busy || !canSave} loading={busy} onclick={save}>Save</Button>
  </DialogFooter>
</Dialog>

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-6);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
  }
  h3 {
    margin: 0;
    font: var(--rt-font-title);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .error {
    display: flex;
    gap: var(--rt-space-3);
    align-items: center;
    color: var(--rt-danger-fg);
  }
  .profiles {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .profiles li {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
  }
  .profile {
    display: flex;
    flex: 1;
    min-width: 0;
    align-items: center;
    gap: var(--rt-space-4);
    padding: var(--rt-space-4) var(--rt-space-5);
    font: var(--rt-font-body);
    color: var(--rt-fg);
    text-align: left;
    background: var(--rt-surface-2);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-md);
    cursor: pointer;
  }
  .profile:hover {
    background: var(--rt-bg-hover);
  }
  .profile.on {
    background: var(--rt-accent-subtle);
    border-color: var(--rt-accent);
  }
  .profile:focus-visible {
    outline: none;
    box-shadow: var(--rt-focus-ring);
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
  }
  .profile .muted {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--rt-space-4);
  }
  code {
    font-family: var(--rt-font-mono);
  }
</style>

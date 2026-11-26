<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The address field of "Clone a repository", shared by the Git panel and the clone dialog.
     A plain text input (not `type="url"`: the browser would refuse paths). Where the backend
     can pick a folder (`git.pickRepository`, desktop) a "Choose folder…" button fills it;
     where it cannot read local paths (browser) a path gets a hint instead of a failure. -->
<script lang="ts">
  import { getGitHost } from "./host";
  import { CLONE_PLACEHOLDER, LOCAL_CLONE_UNSUPPORTED, looksLocal } from "../commands/clone-source";

  let {
    url = $bindable(""),
    disabled = false,
    label = "Clone a repository",
  }: { url: string; disabled?: boolean; label?: string } = $props();
  const host = getGitHost();
  const git = host.backend.git;
  const canPick = typeof git.pickRepository === "function";
  let error = $state<string | null>(null);

  async function choose() {
    error = null;
    try {
      const picked = await git.pickRepository?.();
      if (picked) url = picked;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<label class="field">
  {label}
  <span class="row">
    <input
      data-testid="git-clone-url"
      type="text"
      inputmode="url"
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      placeholder={CLONE_PLACEHOLDER}
      bind:value={url}
      {disabled}
    />
    {#if canPick}
      <button type="button" class="choose" data-testid="git-clone-choose" onclick={choose} {disabled}>Choose folder…</button>
    {/if}
  </span>
</label>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if !canPick && looksLocal(url)}
  <p class="hint warn" data-testid="git-clone-local-hint" role="status">{LOCAL_CLONE_UNSUPPORTED}</p>
{:else}
  <p class="hint">
    A web address or {canPick ? "a folder on this computer (bare or normal repository, also ~/… or file://…)" : "a path (desktop app)"}.
  </p>
{/if}

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  .row {
    display: flex;
    gap: 4px;
    min-width: 0;
  }
  input {
    flex: 1;
    min-width: 0;
    box-sizing: border-box;
    padding: 3px 6px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font: inherit;
  }
  .choose {
    flex: none;
    color: var(--rt-fg);
    white-space: nowrap;
  }
  .hint,
  .error {
    margin: 0;
    font-size: var(--rt-size-small);
    overflow-wrap: anywhere;
  }
  .hint {
    color: var(--rt-fg-muted);
  }
  .warn {
    color: var(--rt-fg);
  }
  .error {
    color: var(--rt-danger, var(--rt-fg));
  }
</style>

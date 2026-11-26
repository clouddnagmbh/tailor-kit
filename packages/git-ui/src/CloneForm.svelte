<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The "Clone a repository" form (wave 15, strand A), shared by the start-screen dialog
     (CloneDialog) and the "Clone repository" tile of the New… dialog (CloneRepositoryDialog).
     The repository lands in the sidebar as a new root (`repositories.clone`); private repositories
     ask for credentials once and retry (`git.setCredentials`). -->
<script lang="ts">
  import { BackendError } from "./api";
  import type { RepositoryEntry } from "./types";
  import { getGitHost } from "./host";
  import CloneFields from "./CloneFields.svelte";

  let {
    oncancel,
    ondone,
  }: {
    oncancel: () => void;
    /** The repository was cloned into a new folder and added to the sidebar. */
    ondone: (entry: RepositoryEntry) => void | Promise<void>;
  } = $props();
  const host = getGitHost();
  const git = host.backend.git;

  let url = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let credentials = $state<string | null>(null);
  let user = $state("");
  let password = $state("");

  async function clone() {
    const address = url.trim();
    if (!address || busy) return;
    busy = true;
    error = null;
    try {
      if (credentials) {
        await git.setCredentials(credentials, user.trim(), password);
        credentials = null;
        password = "";
      }
      // Wave 15: into the sidebar (the folder picker asks where), not instead of it.
      const entry = await host.backend.repositories.clone({ url: address });
      if (entry) await ondone(entry);
    } catch (e) {
      if (e instanceof BackendError && e.kind === "git-auth") {
        credentials = e.message;
        error = `${e.message} needs a user name and a password or access token.`;
      } else {
        error = e instanceof Error ? e.message : String(e);
      }
    } finally {
      busy = false;
    }
  }
</script>

<form
  onsubmit={(e) => {
    e.preventDefault();
    void clone();
  }}
>
  <CloneFields bind:url disabled={busy} label="Repository address" />
  {#if credentials}
    <label>
      User name
      <input data-testid="clone-user" type="text" autocomplete="username" bind:value={user} />
    </label>
    <label>
      Password or token
      <input data-testid="clone-password" type="password" autocomplete="current-password" bind:value={password} />
    </label>
  {/if}
  {#if error}<p class="error" role="alert" data-testid="clone-error">{error}</p>{/if}
  <div class="actions">
    <button type="button" data-testid="clone-cancel" onclick={oncancel}>Cancel</button>
    <button type="submit" data-testid="clone-submit" disabled={!url.trim() || busy}>
      {busy ? "Cloning…" : "Clone"}
    </button>
  </div>
</form>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--rt-gap);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  label input {
    padding: 3px 6px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font: inherit;
  }
  .error {
    margin: 0;
    color: var(--rt-danger);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--rt-gap);
  }
</style>

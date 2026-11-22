<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- What a review needs before it can list anything (wave 12, strand E): the Git host of the
     collection's `origin` (self-hosted hosts are added here, per device) and the user's own
     token for it. The token goes to the keychain (browser: encrypted storage) and is never shown
     again. `github.com` and `gitlab.com` need no host entry. -->
<script lang="ts">
  import { onMount } from "svelte";

  import type { DeviceCodePrompt, ReviewHost, ReviewKind, ReviewRepo } from "../commands/types";
  import { getApp } from "../state/app.svelte";
  import { deviceLoginOffered } from "./model";

  let {
    repo,
    needsToken,
    message,
    onchange,
  }: {
    /** `null`: `origin` is on no host the app knows. */
    repo: ReviewRepo | null;
    needsToken: boolean;
    /** Why the token is asked for again (rejected, expired). */
    message: string | null;
    /** The host or the token changed: look again. */
    onchange: () => void;
  } = $props();

  const app = getApp();

  let hosts = $state<ReviewHost[]>([]);
  let host = $state("");
  let kind = $state<ReviewKind>("github");
  let api = $state("");
  let token = $state("");
  let hasToken = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let prompt = $state<DeviceCodePrompt | null>(null);

  const apiHint = $derived(
    kind === "github" ? "https://git.example.com/api/v3" : "https://gitlab.example.com/api/v4",
  );

  function message_of(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  async function reload() {
    hosts = await app.backend.review.hosts();
    if (repo) hasToken = await app.backend.review.hasToken(repo.host);
  }

  onMount(() => {
    void reload().catch((e: unknown) => (error = message_of(e)));
  });

  async function run(job: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await job();
    } catch (e) {
      error = message_of(e);
    } finally {
      busy = false;
    }
  }

  const addHost = () =>
    run(async () => {
      const entry: ReviewHost = { host: host.trim(), kind, api: api.trim() };
      await app.backend.review.setHosts([...hosts.filter((h) => h.host !== entry.host), entry]);
      host = "";
      api = "";
      await reload();
      onchange();
    });

  const removeHost = (h: ReviewHost) =>
    run(async () => {
      await app.backend.review.setHosts(hosts.filter((x) => x.host !== h.host));
      await reload();
      onchange();
    });

  const saveToken = () =>
    run(async () => {
      if (!repo) return;
      await app.backend.review.setToken(repo.host, token);
      token = "";
      await reload();
      onchange();
    });

  const removeToken = () =>
    run(async () => {
      if (!repo) return;
      await app.backend.review.deleteToken(repo.host);
      await reload();
      onchange();
    });

  const signIn = () =>
    run(async () => {
      if (!repo) return;
      prompt = await app.backend.review.startLogin(repo.host);
      try {
        await app.backend.review.finishLogin(repo.host);
      } finally {
        prompt = null;
      }
      await reload();
      onchange();
    });
</script>

<section class="setup" data-testid="review-setup" aria-label="Pull-request review setup">
  {#if repo === null}
    <p data-testid="review-no-host">
      The origin of this collection is not on a Git host the app knows. <code>github.com</code> and <code>gitlab.com</code> work as they are;
      add a self-hosted GitHub Enterprise or GitLab below. Hosts are kept on this device only, never in the collection.
    </p>
  {/if}

  {#if repo && (needsToken || !hasToken)}
    <form
      class="token"
      onsubmit={(e) => {
        e.preventDefault();
        void saveToken();
      }}
    >
      <h3>Token for {repo.host}</h3>
      {#if message}<p class="error" role="alert" data-testid="review-token-message">{message}</p>{/if}
      <p class="hint">
        Paste a personal access token of yours with permission to read and comment on pull requests
        ({repo.kind === "github" ? "repo scope, or a fine-grained token with pull requests: read and write" : "api scope"}). It is kept
        in the keychain, never in a file, and sent only to {repo.host}.
      </p>
      <div class="row">
        <input
          type="password"
          autocomplete="off"
          spellcheck="false"
          placeholder="Personal access token"
          aria-label="Personal access token"
          data-testid="review-token"
          bind:value={token}
          disabled={busy}
        />
        <button class="button primary" type="submit" data-testid="review-save-token" disabled={busy || token.trim() === ""}>Save token</button>
      </div>
      {#if deviceLoginOffered(repo)}
        <div class="row">
          <button type="button" class="button" data-testid="review-sign-in" disabled={busy} onclick={() => void signIn()}>Sign in with a code instead</button>
          {#if prompt}
            <span data-testid="review-device-code">
              Enter <strong>{prompt.user_code}</strong> at
              <a href={prompt.verification_uri_complete ?? prompt.verification_uri} target="_blank" rel="noopener noreferrer">{prompt.verification_uri}</a>
            </span>
          {/if}
        </div>
      {/if}
    </form>
  {:else if repo}
    <div class="row" data-testid="review-token-set">
      <span>Signed in to {repo.host} with your token.</span>
      <button class="button" data-testid="review-remove-token" disabled={busy} onclick={() => void removeToken()}>Remove token</button>
    </div>
  {/if}

  <h3>Git hosts</h3>
  {#if hosts.length === 0}
    <p class="hint" data-testid="review-no-hosts">None added.</p>
  {:else}
    <ul class="hosts" data-testid="review-hosts">
      {#each hosts as h (h.host)}
        <li>
          <span class="mono">{h.host}</span>
          <span class="muted">{h.kind === "github" ? "GitHub Enterprise" : "GitLab"} · {h.api}</span>
          <button class="button" disabled={busy} aria-label="Remove {h.host}" onclick={() => void removeHost(h)}>Remove</button>
        </li>
      {/each}
    </ul>
  {/if}
  <form
    class="row"
    onsubmit={(e) => {
      e.preventDefault();
      void addHost();
    }}
  >
    <input placeholder="git.example.com" aria-label="Host" data-testid="review-host" bind:value={host} disabled={busy} spellcheck="false" />
    <select aria-label="Kind" data-testid="review-kind" bind:value={kind} disabled={busy}>
      <option value="github">GitHub Enterprise</option>
      <option value="gitlab">GitLab</option>
    </select>
    <input class="wide" placeholder={apiHint} aria-label="API address" data-testid="review-api" bind:value={api} disabled={busy} spellcheck="false" />
    <button class="button" type="submit" data-testid="review-add-host" disabled={busy || host.trim() === "" || api.trim() === ""}>Add host</button>
  </form>

  {#if error}<p class="error" role="alert" data-testid="review-setup-error">{error}</p>{/if}
</section>

<style>
  .setup {
    display: flex;
    flex-direction: column;
    gap: var(--rt-gap);
    padding: var(--rt-gap) 12px;
    max-width: 52rem;
  }
  h3 {
    margin: var(--rt-gap) 0 0;
    font-size: var(--rt-size);
  }
  p {
    margin: 0;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-gap);
  }
  .hint,
  .muted {
    color: var(--rt-fg-muted);
  }
  .hint {
    font-size: var(--rt-size-small);
  }
  .error {
    color: var(--rt-danger);
  }
  .mono {
    font-family: var(--rt-font-mono);
  }
  input,
  select {
    font: inherit;
    color: var(--rt-fg);
    background: var(--rt-bg);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    padding: 2px 6px;
    height: 26px;
  }
  input.wide {
    flex: 1;
    min-width: 14em;
  }
  .token input {
    flex: 1;
    min-width: 16em;
    font-family: var(--rt-font-mono);
  }
  .button {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    cursor: pointer;
  }
  .button.primary:not(:disabled) {
    background: var(--rt-accent);
    border-color: var(--rt-accent);
    color: var(--rt-accent-fg);
  }
  .button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .hosts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .hosts li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-gap);
  }
</style>

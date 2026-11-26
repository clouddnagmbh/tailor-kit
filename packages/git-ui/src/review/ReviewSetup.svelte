<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- What a review needs before it can list anything (wave 12, strand E): the Git host of the
     collection's `origin` (self-hosted hosts are added here, per device) and the user's own
     token for it. The token goes to the keychain (browser: encrypted storage) and is never shown
     again. `github.com` and `gitlab.com` need no host entry. -->
<script lang="ts">
  import { onMount } from "svelte";

  import type { DeviceCodePrompt, ReviewHost, ReviewKind, ReviewRepo } from "../types";
  import { getGitHost } from "../host";
  import { Banner, Button, Card, Select, TextField } from "@tailor-kit/ui";
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

  const gitHost = getGitHost();

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
    hosts = await gitHost.backend.review.hosts();
    if (repo) hasToken = await gitHost.backend.review.hasToken(repo.host);
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
      await gitHost.backend.review.setHosts([...hosts.filter((h) => h.host !== entry.host), entry]);
      host = "";
      api = "";
      await reload();
      onchange();
    });

  const removeHost = (h: ReviewHost) =>
    run(async () => {
      await gitHost.backend.review.setHosts(hosts.filter((x) => x.host !== h.host));
      await reload();
      onchange();
    });

  const saveToken = () =>
    run(async () => {
      if (!repo) return;
      await gitHost.backend.review.setToken(repo.host, token);
      token = "";
      await reload();
      onchange();
    });

  const removeToken = () =>
    run(async () => {
      if (!repo) return;
      await gitHost.backend.review.deleteToken(repo.host);
      await reload();
      onchange();
    });

  const signIn = () =>
    run(async () => {
      if (!repo) return;
      prompt = await gitHost.backend.review.startLogin(repo.host);
      try {
        await gitHost.backend.review.finishLogin(repo.host);
      } finally {
        prompt = null;
      }
      await reload();
      onchange();
    });
</script>

<section class="setup" data-testid="review-setup" aria-label="Pull-request review setup">
  <header>
    <h2>Review setup</h2>
    <p class="muted">Pull and merge requests are read from the Git host of this collection, with your own token.</p>
  </header>

  {#if repo === null}
    <div data-testid="review-no-host">
      <Banner tone="info">
        The origin of this collection is not on a Git host the app knows. <code>github.com</code> and <code>gitlab.com</code> work as they are;
        add a self-hosted GitHub Enterprise or GitLab below. Hosts are kept on this device only, never in the collection.
      </Banner>
    </div>
  {/if}

  {#if repo && (needsToken || !hasToken)}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void saveToken();
      }}
    >
      <Card title="Token for {repo.host}" icon="key">
        <div class="stack">
          {#if message}<div data-testid="review-token-message"><Banner tone="danger">{message}</Banner></div>{/if}
          <p class="hint">
            Paste a personal access token of yours with permission to read and comment on pull requests
            ({repo.kind === "github" ? "repo scope, or a fine-grained token with pull requests: read and write" : "api scope"}). It is kept
            in the keychain, never in a file, and sent only to {repo.host}.
          </p>
          <div class="row">
            <div class="grow">
              <TextField
                type="password"
                mono
                autocomplete="off"
                placeholder="Personal access token"
                aria-label="Personal access token"
                data-testid="review-token"
                bind:value={token}
                disabled={busy}
              />
            </div>
            <Button variant="primary" type="submit" data-testid="review-save-token" disabled={busy || token.trim() === ""}>Save token</Button>
          </div>
          {#if deviceLoginOffered(repo)}
            <div class="row">
              <Button variant="ghost" data-testid="review-sign-in" disabled={busy} onclick={() => void signIn()}>Sign in with a code instead</Button>
              {#if prompt}
                <span data-testid="review-device-code">
                  Enter <strong>{prompt.user_code}</strong> at
                  <a class="link" href={prompt.verification_uri_complete ?? prompt.verification_uri} target="_blank" rel="noopener noreferrer">{prompt.verification_uri}</a>
                </span>
              {/if}
            </div>
          {/if}
        </div>
      </Card>
    </form>
  {:else if repo}
    <div data-testid="review-token-set">
      <Card padding="sm">
        <div class="row">
          <span>Signed in to {repo.host} with your token.</span>
          <span class="spacer"></span>
          <Button variant="ghost" data-testid="review-remove-token" disabled={busy} onclick={() => void removeToken()}>Remove token</Button>
        </div>
      </Card>
    </div>
  {/if}

  <Card title="Git hosts" icon="branch" description="Self-hosted GitHub Enterprise or GitLab, kept on this device.">
    <div class="stack">
      {#if hosts.length === 0}
        <p class="hint" data-testid="review-no-hosts">None added.</p>
      {:else}
        <ul class="hosts" data-testid="review-hosts">
          {#each hosts as h (h.host)}
            <li>
              <span class="mono">{h.host}</span>
              <span class="muted">{h.kind === "github" ? "GitHub Enterprise" : "GitLab"} · {h.api}</span>
              <span class="spacer"></span>
              <Button variant="ghost" size="sm" disabled={busy} aria-label="Remove {h.host}" onclick={() => void removeHost(h)}>Remove</Button>
            </li>
          {/each}
        </ul>
      {/if}
      <form
        class="row add"
        onsubmit={(e) => {
          e.preventDefault();
          void addHost();
        }}
      >
        <TextField placeholder="git.example.com" aria-label="Host" data-testid="review-host" bind:value={host} disabled={busy} />
        <Select
          aria-label="Kind"
          data-testid="review-kind"
          bind:value={kind}
          disabled={busy}
          options={[
            { value: "github", label: "GitHub Enterprise" },
            { value: "gitlab", label: "GitLab" },
          ]}
        />
        <div class="grow">
          <TextField placeholder={apiHint} aria-label="API address" data-testid="review-api" bind:value={api} disabled={busy} />
        </div>
        <Button type="submit" data-testid="review-add-host" disabled={busy || host.trim() === "" || api.trim() === ""}>Add host</Button>
      </form>
    </div>
  </Card>

  {#if error}<div data-testid="review-setup-error"><Banner tone="danger">{error}</Banner></div>{/if}
</section>

<style>
  .setup {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-6);
    max-width: 52rem;
  }
  h2 {
    margin: 0;
    font: var(--rt-font-heading);
  }
  p {
    margin: 0;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-4);
  }
  .grow {
    flex: 1;
    min-width: 14em;
  }
  .spacer {
    flex: 1;
  }
  .hint,
  .muted {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .link {
    color: var(--rt-accent);
  }
  code,
  .mono {
    font-family: var(--rt-font-mono);
  }
  .hosts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-2);
  }
  .hosts li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-4);
  }
</style>

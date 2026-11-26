<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- "Create pull request" (wave 15, strand E): opens a pull request (GitHub) or merge request
     (GitLab) from a branch that is on the server. The branch is not there yet (or the server
     lacks commits): the dialog says so and offers the "Publish branch" wizard first, then goes
     on. The Git host is the one of the repository's `origin`, the token the user's own (the
     Pull requests view asks for it); nothing goes anywhere else. In the dialog slot
     (`AppState.showDialog`), opened with `openNewPull`. -->
<script lang="ts">
  import { onMount, untrack } from "svelte";

  import type { GitRef, NewPullRequest, PullRequest, ReviewRepo } from "../types";
  import { getGitHost, type GitDialogProps } from "../host";
  import { openGitWizard } from "../wizards/open";
  import { openNewPull } from "./open-new-pull";
  import { Banner, Button, Checkbox, DialogFooter, Select, Spinner, TextArea, TextField, type SelectOption } from "@tailor-kit/ui";
  import { defaultBase, localBranches, problem as missing, publishState, pullWord, remoteBranches, titleFromBranch } from "./new-pull";

  let { close, props }: GitDialogProps = $props();
  const host = getGitHost();
  const review = host.backend.review;
  const git = host.backend.git;
  const preset = untrack(() => props as { head?: string; base?: string });

  let loaded = $state(false);
  let refs = $state<GitRef[]>([]);
  let repo = $state<ReviewRepo | null>(null);
  let hasToken = $state(false);
  let head = $state("");
  let base = $state("");
  let title = $state("");
  let body = $state("");
  let draft = $state(false);
  /** The title still is the one the form made from the branch name. */
  let titleTouched = false;
  let busy = $state(false);
  let error = $state<string | null>(null);
  let made = $state<PullRequest | null>(null);

  const word = $derived(pullWord(repo));
  const heads = $derived(localBranches(refs));
  const remote = $derived(remoteBranches(refs));
  const published = $derived(publishState(refs, head));
  const baseOptions = $derived.by<SelectOption[]>(() => {
    const names = remote.length > 0 ? remote : heads.map((r) => r.name);
    const list = names.filter((n) => n !== head);
    if (base && !list.includes(base)) list.unshift(base);
    return list.map((n) => ({ value: n, label: n }));
  });
  const pull = $derived<NewPullRequest>({ title: title.trim(), body, head, base, draft });
  const lacking = $derived(missing(pull, published));

  const describe = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function load() {
    try {
      refs = await git.refs();
      repo = await review.repo();
      hasToken = repo ? await review.hasToken(repo.host) : false;
      const current = heads.find((r) => r.current)?.name ?? heads[0]?.name ?? "";
      head = preset.head && heads.some((r) => r.name === preset.head) ? preset.head : current;
      base = preset.base ?? defaultBase(remote, heads.map((r) => r.name), head);
      title = titleFromBranch(head);
    } catch (e) {
      error = describe(e);
    } finally {
      loaded = true;
    }
  }

  onMount(() => void load());

  function pickHead(value: string) {
    head = value;
    if (!titleTouched) title = titleFromBranch(value);
    if (base === value || base === "") base = defaultBase(remote, heads.map((r) => r.name), value);
  }

  async function publish() {
    error = null;
    const done = await openGitWizard(host, { kind: "publish", preset: { name: head } });
    // The wizard replaced this dialog; open it again on the same branch if it published.
    if (done) await openNewPull(host, { head, base });
  }

  async function create() {
    if (lacking || busy) return;
    busy = true;
    error = null;
    try {
      made = await review.create(pull);
    } catch (e) {
      error = describe(e);
      const kind = (e as { kind?: string }).kind;
      if (kind === "no-token" || kind === "auth") hasToken = false;
    } finally {
      busy = false;
    }
  }

  function seeAll() {
    close(made ?? undefined);
    host.show({ kind: "review", number: made?.number });
  }

  function setup() {
    close();
    host.show({ kind: "review" });
  }
</script>

<div class="new-pull" data-testid="new-pull">
  {#if !loaded}
    <div class="waiting"><Spinner size="sm" /> <span class="muted">Reading the repository…</span></div>
  {:else if made}
    <section class="done" data-testid="new-pull-done" aria-live="polite">
      <p class="lead">Opened {word} <strong>#{made.number}</strong>: {made.title}</p>
      <p class="muted"><a href={made.url} target="_blank" rel="noreferrer noopener" data-testid="new-pull-link">{made.url}</a></p>
    </section>
    <DialogFooter>
      <Button data-testid="new-pull-close" onclick={() => close(made ?? undefined)}>Done</Button>
      <Button variant="primary" data-testid="new-pull-show" onclick={seeAll}>Show in list</Button>
    </DialogFooter>
  {:else if !repo}
    <Banner tone="warn" title="This repository's server is not known">
      Opening a {word} needs a repository whose <code>origin</code> is on GitHub or GitLab (a self-hosted one can be added under “Token and hosts” in the Pull requests view).
    </Banner>
    <DialogFooter>
      <Button data-testid="new-pull-cancel" onclick={() => close()}>Close</Button>
      <Button variant="primary" data-testid="new-pull-setup" onclick={setup}>Open pull requests</Button>
    </DialogFooter>
  {:else}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <p class="muted" data-testid="new-pull-repo">{repo.host} · {repo.project}</p>
      <div class="branches">
        <Select label="Your changes" data-testid="new-pull-head" value={head} onchange={(e: Event) => pickHead((e.currentTarget as HTMLSelectElement).value)} options={heads.map((r) => ({ value: r.name, label: r.current ? `${r.name} (current)` : r.name }))} />
        <Select label="Merge into" data-testid="new-pull-base" bind:value={base} options={baseOptions} />
      </div>

      {#if head && published !== "published"}
        <div data-testid="new-pull-unpublished" data-state={published}>
          <Banner tone="warn" title={published === "unpublished" ? `${head} is not on the server yet` : `The server has fewer commits of ${head}`}>
            A {word} shows what is on the server. {published === "unpublished" ? "Publish the branch first." : "Send your commits first."}
            {#snippet actions()}
              <Button size="sm" variant="primary" icon="push" data-testid="new-pull-publish" onclick={publish}>{published === "unpublished" ? "Publish branch…" : "Send changes…"}</Button>
            {/snippet}
          </Banner>
        </div>
      {/if}

      {#if !hasToken}
        <div data-testid="new-pull-no-token">
          <Banner tone="info" title="Sign in to {repo.host} first">
            Your own access token is needed. It stays on this device.
            {#snippet actions()}
              <Button size="sm" data-testid="new-pull-setup" onclick={setup}>Sign in…</Button>
            {/snippet}
          </Banner>
        </div>
      {/if}

      <TextField
        label="Title"
        data-testid="new-pull-title"
        bind:value={title}
        oninput={() => (titleTouched = true)}
      />
      <TextArea label="Description" rows={4} data-testid="new-pull-body" bind:value={body} />
      <Checkbox label="Open as a draft" description="A draft says it is not ready for review yet." data-testid="new-pull-draft" bind:checked={draft} />

      {#if error}<div data-testid="new-pull-error"><Banner tone="danger">{error}</Banner></div>{/if}
      <DialogFooter>
        <Button type="button" data-testid="new-pull-cancel" onclick={() => close()}>Cancel</Button>
        <Button type="submit" variant="primary" icon="review" data-testid="new-pull-create" loading={busy} disabled={Boolean(lacking) || busy || !hasToken} title={lacking ?? undefined}>
          Create {word}
        </Button>
      </DialogFooter>
    </form>
  {/if}
</div>

<style>
  .new-pull,
  form {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
  }
  .branches {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--rt-space-4);
  }
  .muted {
    margin: 0;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .lead {
    margin: 0;
  }
  .waiting {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
  }
  a {
    color: var(--rt-accent);
    overflow-wrap: anywhere;
  }
</style>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Sidebar view "Git": status, diff, commit, branches, push/pull, conflicts, clone.
     Owner: wave 4, strand A2. `onopen` shows a collection the backend has opened (clone).
     Talks to Git only through `app.backend.git`, so it is the same in both shells.
     - Re-reads the status when it opens, after every Git action, after a save (`app.dirty`)
       and when the collection is (re)loaded (`app.loadCount`).
     - No staging area: every changed file has a checkbox (checked unless the user unchecked
       it); Commit commits the checked ones.
     - Switching branches, pulling and resolving change files: they wait for unsaved changes
       to be saved, and the view says so. Afterwards the app re-reads the collection.
     - A command rejected with kind `git-auth` (message: the host) asks for credentials,
       stores them and runs the command again. The first commit (or pull) asks for the author
       identity if none is set.
     - Before a commit the secret scanner checks the checked files (`git.scan`, wave 5): with
       findings nothing is committed and SecretFindings lists them (file, line, hint). A
       commit the backend refuses with `secrets-found` shows them the same way.
     Wave 12: owner strand A (semantic diff in the changes and the log). The "Tools" section
     lists the palette entries of all wave 12 strands (`api-commands.ts`); it is interface. -->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { BackendError, type CollectionInfo } from "../commands/backend";
  import type { GitBranch, GitCommit, GitFile, GitResolution, GitStatus, SecretFinding } from "../commands/types";
  import { getApp } from "../state/app.svelte";
  import { gitApiCommands } from "./api-commands";
  import ConflictView from "./ConflictView.svelte";
  import DiffView from "./DiffView.svelte";
  import SecretFindings from "./SecretFindings.svelte";
  import { getGitViews } from "./views.svelte";
  import {
    ago,
    branchLabel,
    commitMessage,
    mergeMessage,
    pullMessage,
    pushMessage,
    STATE_LABEL,
    STATE_LETTER,
    syncLabel,
  } from "./format";

  let { onopen }: { onopen: (info: CollectionInfo) => Promise<void> } = $props();
  const app = getApp();
  const views = getGitViews();
  const tools = $derived(gitApiCommands(app, views));
  const git = app.backend.git;

  let status = $state<GitStatus | null>(null);
  let branches = $state<GitBranch[]>([]);
  let commits = $state<GitCommit[]>([]);
  /** Paths the user unchecked. */
  let excluded = $state<string[]>([]);
  let message = $state("");
  /** The merge whose message was pre-filled, so it is filled once. */
  let prefilledFor = $state<string | null>(null);
  /** Running command, for progress and to disable the buttons. */
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let result = $state<string | null>(null);
  /** Credentials that blocked the last commit attempt; cleared when files change. */
  let findings = $state<SecretFinding[]>([]);
  let loadError = $state<string | null>(null);

  /** Waiting for the identity, then runs `then`. */
  let identityThen = $state<(() => Promise<void>) | null>(null);
  let identityName = $state("");
  let identityEmail = $state("");
  /** Waiting for credentials for `host`, then retries. */
  let credentials = $state<{ host: string; retry: () => Promise<void> } | null>(null);
  let credUser = $state("");
  let credPassword = $state("");

  let diffPath = $state<string | null>(null);
  let conflictPath = $state<string | null>(null);
  let opener: HTMLElement | null = null;

  let creatingBranch = $state(false);
  let newBranch = $state("");
  let cloneUrl = $state("");
  let cloneOpen = $state(false);
  let branchSelect = $state<HTMLSelectElement>();

  const files = $derived(status?.files ?? []);
  const selected = $derived(files.filter((f) => !excluded.includes(f.path)).map((f) => f.path));
  const conflicted = $derived(files.some((f) => f.state === "conflicted"));
  const blocked = $derived(
    app.dirty.length > 0 ? "Save your changes first: switching branches, pulling and resolving change files." : null,
  );
  const localBranches = $derived(branches.filter((b) => !b.remote));
  const remoteBranches = $derived(
    branches.filter((b) => b.remote && !localBranches.some((l) => b.name.endsWith(`/${l.name}`))),
  );
  /** Branches the current one can be compared with (semantic diff, `between`). */
  const compareTargets = $derived(branches.filter((b) => b.name !== status?.branch));
  const canCommit = $derived(!busy && !conflicted && message.trim() !== "" && selected.length > 0);

  const describe = (e: unknown) => (e instanceof Error ? e.message : String(e));

  let generation = 0;
  async function refresh(): Promise<void> {
    if (!app.collection) return;
    const mine = ++generation;
    try {
      const s = await git.status();
      const [b, c] = s.repo ? await Promise.all([git.branches(), git.log(5)]) : [[], []];
      if (mine !== generation) return;
      status = s;
      branches = b;
      commits = c;
      loadError = null;
      excluded = excluded.filter((p) => s.files.some((f) => f.path === p));
      if (s.merging && prefilledFor !== `${s.branch}:${s.upstream}`) {
        prefilledFor = `${s.branch}:${s.upstream}`;
        if (!message.trim()) message = mergeMessage(s);
      }
      if (!s.merging) prefilledFor = null;
    } catch (e) {
      if (mine === generation) loadError = describe(e);
    }
  }

  // On open, on every (re)load of the collection and whenever the set of unsaved files
  // changes (a save clears it).
  const dirtyKey = $derived(app.dirty.join("\n"));
  $effect(() => {
    void app.loadCount;
    void dirtyKey;
    void app.collection;
    untrack(() => {
      findings = [];
      void refresh();
    });
  });

  /** Opens the branch compare: the current branch → the chosen one. */
  function compareWith(e: Event & { currentTarget: HTMLSelectElement }): void {
    const to = e.currentTarget.value;
    e.currentTarget.value = "";
    if (to) views.open({ kind: "diff", scope: { kind: "between", from: status?.branch ?? "HEAD", to } });
  }

  /** Runs a Git command with progress, error display and the credentials prompt. */
  async function act(name: string, run: () => Promise<void>): Promise<void> {
    busy = name;
    error = null;
    result = null;
    try {
      await run();
    } catch (e) {
      if (e instanceof BackendError && e.kind === "git-auth") {
        credentials = { host: e.message, retry: () => act(name, run) };
        credUser = "";
        credPassword = "";
        await tick();
        document.querySelector<HTMLInputElement>('[data-testid="git-cred-username"]')?.focus();
      } else {
        error = describe(e);
      }
    } finally {
      busy = null;
      await refresh();
      if (branchSelect && status?.branch) branchSelect.value = status.branch;
    }
  }

  /** Runs `then` once an identity is set (asks for it first if needed). */
  async function withIdentity(then: () => Promise<void>): Promise<void> {
    try {
      if (await git.identity()) return then();
    } catch (e) {
      error = describe(e);
      return;
    }
    identityThen = then;
    await tick();
    document.querySelector<HTMLInputElement>('[data-testid="git-identity-name"]')?.focus();
  }

  async function saveIdentity() {
    error = null;
    try {
      await git.setIdentity({ name: identityName, email: identityEmail });
    } catch (e) {
      error = describe(e);
      return;
    }
    const then = identityThen;
    identityThen = null;
    if (then) await then();
  }

  async function saveCredentials() {
    const c = credentials;
    if (!c) return;
    error = null;
    try {
      await git.setCredentials(c.host, credUser, credPassword);
    } catch (e) {
      error = describe(e);
      return;
    }
    credentials = null;
    credPassword = "";
    await c.retry();
  }

  function toggle(path: string, checked: boolean) {
    excluded = checked ? excluded.filter((p) => p !== path) : [...excluded, path];
  }

  function toggleAll(checked: boolean) {
    excluded = checked ? [] : files.map((f) => f.path);
  }

  const commit = () =>
    withIdentity(() =>
      act("commit", async () => {
        const paths = selected;
        findings = await git.scan(paths);
        if (findings.length > 0) return;
        try {
          await git.commit(message, paths);
        } catch (e) {
          if (e instanceof BackendError && e.kind === "secrets-found") {
            findings = await git.scan(paths);
            if (findings.length > 0) return;
          }
          throw e;
        }
        message = "";
        result = commitMessage(paths.length);
      }),
    );

  const push = () =>
    act("push", async () => {
      await git.push();
      result = pushMessage(status);
    });

  const pull = () =>
    withIdentity(() =>
      act("pull", async () => {
        const upstream = status?.upstream ?? null;
        const r = await git.pull();
        if (r.outcome !== "up-to-date") await app.reload();
        result = pullMessage(r, upstream);
      }),
    );

  const checkout = (branch: string) =>
    act("checkout", async () => {
      await git.checkout(branch);
      await app.reload();
      result = `Switched to ${branch}.`;
    });

  const createBranch = () =>
    act("branch", async () => {
      const name = newBranch.trim();
      await git.createBranch(name, true);
      newBranch = "";
      creatingBranch = false;
      result = `Created and switched to ${name}.`;
    });

  const init = () =>
    act("init", async () => {
      await git.init();
      result = "Repository created. Commit the files to start its history.";
    });

  const clone = () =>
    act("clone", async () => {
      const info = await git.clone(cloneUrl.trim());
      if (!info) return;
      cloneUrl = "";
      cloneOpen = false;
      await onopen(info);
      result = `Cloned into ${info.location}.`;
    });

  const resolve = (path: string, resolution: GitResolution) =>
    act("resolve", async () => {
      await git.resolve(path, resolution);
      closeDialog();
      await app.reload();
      result = `Kept ${resolution === "ours" ? "our" : "their"} version of ${path}.`;
    });

  function openFile(f: GitFile, e: Event) {
    opener = e.currentTarget as HTMLElement;
    if (f.state === "conflicted") conflictPath = f.path;
    else diffPath = f.path;
  }

  function closeDialog() {
    diffPath = null;
    conflictPath = null;
    const back = opener;
    opener = null;
    void tick().then(() => back?.isConnected && back.focus());
  }
</script>

<div class="git" data-testid="git-panel" aria-busy={busy !== null}>
  {#if credentials}
    <form
      class="card"
      data-testid="git-credentials"
      aria-label="Credentials for {credentials.host}"
      onsubmit={(e) => {
        e.preventDefault();
        void saveCredentials();
      }}
    >
      <p><strong>{credentials.host}</strong> needs a username and password (or access token).</p>
      <label>Username <input data-testid="git-cred-username" autocomplete="username" bind:value={credUser} /></label>
      <label>
        Password or token
        <input data-testid="git-cred-password" type="password" autocomplete="current-password" bind:value={credPassword} />
      </label>
      <p class="hint">
        Stored on this device ({app.backend.kind === "web"
          ? "encrypted in this browser"
          : app.backend.kind === "tauri"
            ? "in the system keychain"
            : "in memory"}), never in the collection.
      </p>
      <div class="row">
        <button class="primary" data-testid="git-cred-save" type="submit" disabled={!credUser || !credPassword}>
          Save and retry
        </button>
        <button type="button" data-testid="git-cred-cancel" onclick={() => (credentials = null)}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if identityThen}
    <form
      class="card"
      data-testid="git-identity"
      aria-label="Author for commits"
      novalidate
      onsubmit={(e) => {
        e.preventDefault();
        void saveIdentity();
      }}
    >
      <p>Who is committing? Name and email go into every commit.</p>
      <label>Name <input data-testid="git-identity-name" autocomplete="name" bind:value={identityName} /></label>
      <label>Email <input data-testid="git-identity-email" type="email" autocomplete="email" bind:value={identityEmail} /></label>
      <div class="row">
        <button class="primary" data-testid="git-identity-save" type="submit">Save</button>
        <button type="button" onclick={() => (identityThen = null)}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if error}<p class="error" role="alert" data-testid="git-error">{error}</p>{/if}
  {#if result}<p class="result" role="status" data-testid="git-result">{result}</p>{/if}
  {#if busy}<p class="progress" role="status" data-testid="git-progress">{
        { push: "Pushing…", pull: "Pulling…", clone: "Cloning…", commit: "Committing…", checkout: "Switching…" }[busy] ??
        "Working…"
      }</p>{/if}

  {#if loadError}
    <p class="error" role="alert" data-testid="git-load-error">{loadError}</p>
  {:else if !status}
    <p class="hint">Reading the repository…</p>
  {:else if !status.repo}
    <section class="card" data-testid="git-norepo">
      <p>This collection is not in a Git repository.</p>
      {#if app.backend.kind === "web"}
        <p class="hint">In the browser, the repository must be the collection folder itself: the app cannot see the folders above it.</p>
      {/if}
      <button class="primary" data-testid="git-init" disabled={busy !== null} onclick={init}>Initialize repository</button>
    </section>
  {:else}
    <section class="head" aria-label="Branch">
      <div class="branch-line">
        <span class="label">Branch</span>
        <strong data-testid="git-branch">{branchLabel(status, branches)}</strong>
      </div>
      <div class="sync" data-testid="git-sync">{syncLabel(status)}</div>
      <div class="row">
        <select
          bind:this={branchSelect}
          data-testid="git-branch-select"
          aria-label="Switch branch"
          value={status.branch ?? ""}
          disabled={!!blocked || busy !== null || status.merging || !status.branch}
          onchange={(e) => {
            const v = e.currentTarget.value;
            if (v && v !== status?.branch) void checkout(v);
          }}
        >
          {#if !status.branch}<option value="">{branchLabel(status, branches)}</option>{/if}
          {#each localBranches as b (b.name)}
            <option value={b.name}>{b.name}</option>
          {/each}
          {#if remoteBranches.length > 0}
            <optgroup label="Remote">
              {#each remoteBranches as b (b.name)}
                <option value={b.name}>{b.name}</option>
              {/each}
            </optgroup>
          {/if}
        </select>
        <button data-testid="git-new-branch" disabled={busy !== null} onclick={() => (creatingBranch = !creatingBranch)}>
          New branch
        </button>
      </div>
      {#if creatingBranch}
        <form
          class="row"
          onsubmit={(e) => {
            e.preventDefault();
            void createBranch();
          }}
        >
          <!-- svelte-ignore a11y_autofocus -->
          <input data-testid="git-new-branch-name" aria-label="New branch name" placeholder="feature/name" bind:value={newBranch} autofocus />
          <button data-testid="git-create-branch" type="submit" disabled={!newBranch.trim() || busy !== null}>Create</button>
        </form>
      {/if}
      <div class="row">
        <button data-testid="git-pull" disabled={!!blocked || busy !== null || status.merging} onclick={pull}>
          {busy === "pull" ? "Pulling…" : status.behind > 0 ? `Pull ↓${status.behind}` : "Pull"}
        </button>
        <button data-testid="git-push" disabled={busy !== null || status.merging || !status.branch} onclick={push}>
          {busy === "push" ? "Pushing…" : status.ahead > 0 ? `Push ↑${status.ahead}` : "Push"}
        </button>
      </div>
      {#if blocked}<p class="warn" data-testid="git-blocked">{blocked}</p>{/if}
    </section>

    {#if status.merging}
      <p class="warn card" data-testid="git-merging">
        Merge in progress. {conflicted ? "Open each conflicted file (C) and choose a side, then commit." : "Commit to finish the merge."}
      </p>
    {/if}

    <section class="changes" aria-label="Changes">
      {#if files.length === 0}
        <p class="hint" data-testid="git-clean">No changes: everything is committed.</p>
      {:else}
        <label class="all">
          <input
            type="checkbox"
            data-testid="git-select-all"
            checked={selected.length === files.length}
            indeterminate={selected.length > 0 && selected.length < files.length}
            onchange={(e) => toggleAll(e.currentTarget.checked)}
          />
          {files.length} changed {files.length === 1 ? "file" : "files"}
        </label>
        <button class="link" data-testid="git-semantic-diff" onclick={() => views.open({ kind: "diff", scope: { kind: "working" } })}>Show changes by field</button>
        <ul>
          {#each files as f (f.path)}
            <li data-testid="git-file">
              <input
                type="checkbox"
                data-testid="git-file-check"
                aria-label="Include {f.path}"
                checked={!excluded.includes(f.path)}
                onchange={(e) => toggle(f.path, e.currentTarget.checked)}
              />
              <button class="file" data-testid="git-file-open" title="{f.path}: {STATE_LABEL[f.state]}" onclick={(e) => openFile(f, e)}>
                <span class="path">{f.path}</span>
                <span class="state {f.state}" data-testid="git-file-state" aria-label={STATE_LABEL[f.state]}>{STATE_LETTER[f.state]}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <form
        class="commit"
        onsubmit={(e) => {
          e.preventDefault();
          if (canCommit) void commit();
        }}
      >
        <textarea data-testid="git-message" aria-label="Commit message" placeholder="Commit message" rows="3" bind:value={message}></textarea>
        <button class="primary" data-testid="git-commit" type="submit" disabled={!canCommit}>
          {busy === "commit" ? "Committing…" : selected.length > 0 ? `Commit ${selected.length} ${selected.length === 1 ? "file" : "files"}` : "Commit"}
        </button>
      </form>
      {#if findings.length > 0}<SecretFindings {findings} />{/if}
    </section>

    {#if tools.length > 0}
      <section class="tools" aria-label="Tools">
        <h3>Tools</h3>
        <ul data-testid="git-tools">
          {#each tools as t (t.id)}
            <li><button class="link" data-testid="git-tool-{t.id}" onclick={() => t.run()}>{t.label}</button></li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if commits.length > 0}
      <section class="log" aria-label="Recent commits">
        <h3>Recent commits</h3>
        <ol data-testid="git-log">
          {#each commits as c (c.oid)}
            <li title="{c.oid.slice(0, 10)} by {c.author}, {ago(c.time)}">
              <button class="link commit-link" data-testid="git-commit-changes" onclick={() => views.open({ kind: "diff", scope: { kind: "commit", rev: c.oid } })}>{c.summary}</button>
            </li>
          {/each}
        </ol>
        {#if compareTargets.length > 0}
          <label class="compare">
            Compare with
            <select data-testid="git-compare-branch" onchange={compareWith}>
              <option value="">Branch…</option>
              {#each compareTargets as b (b.name)}<option value={b.name}>{b.name}</option>{/each}
            </select>
          </label>
        {/if}
      </section>
    {/if}
  {/if}

  {#if status && (!status.repo || cloneOpen)}
    <form
      class="card clone"
      aria-label="Clone a repository"
      onsubmit={(e) => {
        e.preventDefault();
        if (cloneUrl.trim()) void clone();
      }}
    >
      <label>
        Clone a repository
        <input data-testid="git-clone-url" type="url" placeholder="https://github.com/acme/api-tests.git" bind:value={cloneUrl} />
      </label>
      <p class="hint">Opens the clone as the collection{app.backend.kind === "web" ? "; pick the folder to clone into" : ""}.</p>
      <button data-testid="git-clone" type="submit" disabled={!cloneUrl.trim() || busy !== null}>
        {busy === "clone" ? "Cloning…" : "Clone"}
      </button>
    </form>
  {:else if status}
    <button class="link" data-testid="git-clone-open" onclick={() => (cloneOpen = true)}>Clone another repository…</button>
  {/if}
</div>

{#if diffPath}
  <DiffView path={diffPath} onclose={closeDialog} />
{/if}
{#if conflictPath}
  <ConflictView
    path={conflictPath}
    {blocked}
    onresolve={(r) => conflictPath && resolve(conflictPath, r)}
    onclose={closeDialog}
  />
{/if}

<style>
  .git {
    height: 100%;
    overflow: auto;
    padding: var(--rt-gap);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: var(--rt-gap);
  }
  p {
    margin: 0;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: var(--rt-gap);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg-panel);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  input:not([type="checkbox"]),
  textarea,
  select {
    min-width: 0;
    box-sizing: border-box;
    padding: 3px 6px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font-size: var(--rt-size);
  }
  textarea {
    width: 100%;
    resize: vertical;
  }
  button {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    cursor: pointer;
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    background: var(--rt-bg-hover);
  }
  button:disabled,
  select:disabled {
    color: var(--rt-fg-muted);
    cursor: default;
    opacity: 0.6;
  }
  button.primary:not(:disabled) {
    background: var(--rt-accent);
    border-color: var(--rt-accent);
    color: var(--rt-accent-fg);
  }
  button.link {
    align-self: flex-start;
    border: none;
    background: none;
    padding: 0;
    color: var(--rt-accent);
    font-size: var(--rt-size-small);
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row select,
  .row input {
    flex: 1;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .branch-line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .branch-line strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label,
  .sync,
  .hint,
  h3 {
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  h3 {
    margin: 0 0 4px;
    font-weight: 600;
  }
  .changes {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .all {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  ul,
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li[data-testid="git-file"] {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .file {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 6px;
    justify-content: space-between;
    height: auto;
    padding: 2px 4px;
    border: none;
    background: none;
    text-align: left;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    flex: none;
    font-weight: 700;
  }
  .state.untracked {
    color: var(--rt-ok);
  }
  .state.modified {
    color: var(--rt-warn);
  }
  .state.deleted,
  .state.conflicted {
    color: var(--rt-danger);
  }
  .commit {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 4px;
  }
  .commit button {
    align-self: flex-start;
  }
  .tools ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .tools .link {
    background: none;
    border: none;
    padding: 2px 0;
    color: var(--rt-accent);
    cursor: pointer;
    font: inherit;
    text-align: left;
  }
  .link {
    background: none;
    border: none;
    padding: 2px 0;
    color: var(--rt-accent);
    cursor: pointer;
    font: inherit;
    text-align: left;
  }
  .commit-link {
    display: block;
    width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--rt-size-small);
    padding: 0;
  }
  .compare {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    margin-top: 4px;
    font-size: var(--rt-size-small);
  }
  .log li {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--rt-size-small);
    padding: 1px 0;
  }
  .error {
    color: var(--rt-danger);
  }
  .warn {
    color: var(--rt-warn);
    font-size: var(--rt-size-small);
  }
  .result {
    color: var(--rt-ok);
  }
  .progress {
    color: var(--rt-fg-muted);
  }
</style>

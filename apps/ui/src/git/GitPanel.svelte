<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Sidebar view "Git" (wave 14, strand G; first built in wave 4). Status at a glance, top down:
     - Repository card: name, the current branch as a chip with its menu (switch, new branch,
       merge into current, rename, delete: the wizards), one sync line ("↑1 ↓2", "Up to date",
       "Not published") and a single contextual action ("Pull 2", "Push 1", "Sync…",
       "Publish branch"); the other sync actions and "Undo last Git action" in its menu.
     - A banner for the situations that need a decision: conflicts, a merge to finish,
       branches that went apart, not on a branch. Each says what happened in plain words and
       offers the ways out; "What does this mean?" explains the Git word (`glossary.ts`).
     - Changes: the changed requests and files with A/M/D/C badges and the request method; a
       click shows the change. The commit area appears only when there is something to
       commit: message, "Commit 3 changes" and "Commit & push" in its split menu; parking and
       discarding in the section's menu.
     - Tools: the Git views as tiles (graph, compare, history, bisect, review, contract,
       secrets), each with its explanation as tooltip.
     - Recent commits: a mini lane graph with branch and tag labels; each commit's menu holds
       its actions; "Open graph" opens the whole graph.
     Rules kept from wave 4: no staging area (every file has a checkbox, checked unless the user
     unchecked it); what changes files waits for unsaved edits to be saved; `git-auth` asks for
     credentials and retries; the first commit asks for the author; the secret scan runs
     before each commit (`SecretFindings`). Talks to Git only through `app.backend.git`. -->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { BackendError, type CollectionInfo } from "../commands/backend";
  import type {
    GitActionResult,
    GitBranch,
    GitCommit,
    GitGraphCommit,
    GitRef,
    GitRemote,
    GitResolution,
    GitStash,
    GitStatus,
    GitUndoEntry,
    SecretFinding,
  } from "../commands/types";
  import { getApp } from "../state/app.svelte";
  import { gitApiCommands } from "./api-commands";
  import CloneFields from "./CloneFields.svelte";
  import ConflictView from "./ConflictView.svelte";
  import DiffView from "./DiffView.svelte";
  import { commitMessage, mergeMessage, pullMessage, pushMessage } from "./format";
  import { GLOSSARY, type GitTerm } from "./glossary";
  import {
    changeRows,
    commitLabel,
    repoName,
    situation,
    syncAction,
    syncState,
    toolTiles,
    type GitSituation,
    type SyncActionKind,
  } from "./panel/model";
  import RecentCommits from "./panel/RecentCommits.svelte";
  import ToolsGrid from "./panel/ToolsGrid.svelte";
  import SecretFindings from "./SecretFindings.svelte";
  import {
    Badge,
    Banner,
    Button,
    Card,
    Checkbox,
    Chip,
    Dropdown,
    EmptyState,
    Icon,
    IconButton,
    Progress,
    Section,
    Spinner,
    SplitButton,
    TextArea,
    TextField,
    tooltip,
    type MenuEntry,
  } from "../ui";
  import { getGitViews } from "./views.svelte";
  import { openGitWizard, type GitWizardKind } from "./wizards/open";

  let { onopen }: { onopen: (info: CollectionInfo) => Promise<void> } = $props();
  const app = getApp();
  const views = getGitViews();
  const commands = $derived(gitApiCommands(app, views));
  const git = app.backend.git;
  /** Commits loaded for the panel; it shows as many as fit its height. */
  const RECENT = 40;

  let status = $state<GitStatus | null>(null);
  let branches = $state<GitBranch[]>([]);
  let recent = $state<GitGraphCommit[]>([]);
  let refs = $state<GitRef[]>([]);
  let head = $state<string | null>(null);
  /** `null`: the shell cannot list remotes yet. */
  let remotes = $state<GitRemote[] | null>(null);
  let stashes = $state<GitStash[]>([]);
  let last = $state<GitUndoEntry | null>(null);
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
  /** The Git word whose explanation is open ("What does this mean?"). */
  let explaining = $state<GitTerm | null>(null);

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

  let cloneUrl = $state("");
  let cloneOpen = $state(false);

  const files = $derived(status?.files ?? []);
  const rows = $derived(changeRows(files, app.tree));
  const selected = $derived(files.filter((f) => !excluded.includes(f.path)).map((f) => f.path));
  const conflicted = $derived(files.some((f) => f.state === "conflicted"));
  const blocked = $derived(
    app.dirty.length > 0 ? "Save your changes first: switching branches, pulling and resolving change files." : null,
  );
  const sit = $derived<GitSituation>(situation(status, branches, recent.length > 0, remotes ? remotes.length : null));
  const sync = $derived(syncState(sit));
  const primary = $derived(syncAction(sit));
  const localBranches = $derived(branches.filter((b) => !b.remote));
  const remoteOnly = $derived(
    branches.filter((b) => b.remote && !localBranches.some((l) => b.name.endsWith(`/${l.name}`)) && !b.name.endsWith("/HEAD")),
  );
  const current = $derived(status?.branch ?? null);
  /** Branches to compare the current one with, field by field ("Compare with" in the branch menu). */
  const compareTargets = $derived(branches.filter((b) => b.name !== current && !b.name.endsWith("/HEAD")));
  const origin = $derived(remotes?.find((r) => r.name === "origin") ?? remotes?.[0] ?? null);
  const name = $derived(repoName(origin?.url ?? null, app.collection?.location ?? null));
  /** Other collections of the set in the same repository (strand A fills `repo`). */
  const siblings = $derived.by(() => {
    const mine = app.collections.find((c) => c.id === app.activeCollection);
    return mine?.repo ? app.collections.filter((c) => c.repo === mine.repo && c.id !== mine.id) : [];
  });
  const canCommit = $derived(!busy && !conflicted && message.trim() !== "" && selected.length > 0);
  const tiles = $derived(toolTiles(commands, () => views.open({ kind: "graph" })));

  const describe = (e: unknown) => (e instanceof Error ? e.message : String(e));

  /** Refs for the labels when the shell has no graph yet: branches by name. */
  async function refsFromBranches(list: GitBranch[]): Promise<GitRef[]> {
    const out: GitRef[] = [];
    for (const b of list) {
      try {
        const target = await git.revParse(b.name);
        out.push({
          full: b.remote ? `refs/remotes/${b.name}` : `refs/heads/${b.name}`,
          name: b.name,
          kind: b.remote ? "remote" : "local",
          target,
          current: b.current,
          ahead: 0,
          behind: 0,
        });
      } catch {
        // A branch without commits has no target.
      }
    }
    return out;
  }

  /** The recent commits with parents: the graph's first page, or the branch's log as one lane. */
  async function loadRecent(): Promise<{ commits: GitGraphCommit[]; refs: GitRef[] | null; head: string | null }> {
    try {
      const page = await git.graph({ skip: 0, limit: RECENT });
      return { commits: page.commits, refs: page.refs, head: page.head ?? null };
    } catch {
      const log: GitCommit[] = await git.log(RECENT);
      return {
        commits: log.map((c, i) => ({ ...c, parents: log[i + 1] ? [log[i + 1].oid] : [], email: "" })),
        refs: null,
        head: log[0]?.oid ?? null,
      };
    }
  }

  let generation = 0;
  async function refresh(): Promise<void> {
    if (!app.collection) return;
    const mine = ++generation;
    try {
      const s = await git.status();
      if (!s.repo) {
        if (mine !== generation) return;
        status = s;
        branches = [];
        recent = [];
        refs = [];
        loadError = null;
        return;
      }
      const [b, r, rem, st, la] = await Promise.all([
        git.branches(),
        loadRecent(),
        git.remotes().catch(() => null),
        git.stashes().catch(() => [] as GitStash[]),
        git.lastAction().catch(() => null),
      ]);
      const rf = r.refs ?? (await refsFromBranches(b));
      if (mine !== generation) return;
      status = s;
      branches = b;
      recent = r.commits;
      refs = rf;
      head = r.head;
      remotes = rem;
      stashes = st;
      last = la;
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

  // A wizard opened from the palette changes Git behind the panel's back: read again when a
  // dialog closes.
  const dialogOpen = $derived(app.dialog !== null);
  let wasOpen = false;
  $effect(() => {
    const open = dialogOpen;
    if (wasOpen && !open) untrack(() => void refresh());
    wasOpen = open;
  });

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
    }
  }

  /** Opens a wizard; afterwards reloads what it changed and shows what it did. */
  async function wizard(kind: GitWizardKind, preset?: Record<string, unknown>): Promise<void> {
    error = null;
    result = null;
    // The wizard reloads the collections itself when the action changed files.
    const done: GitActionResult | undefined = await openGitWizard(app, { kind, preset });
    if (done) result = done.message;
    await refresh();
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

  const commit = (andPush = false) =>
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
        if (andPush) {
          busy = "push";
          const s = await git.status();
          await git.push();
          result = `${commitMessage(paths.length)} ${pushMessage(s)}`;
        }
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
        // Conflicts: the banner says what to do.
        result = r.outcome === "conflicts" ? null : pullMessage(r, upstream);
      }),
    );

  const checkout = (branch: string) =>
    act("checkout", async () => {
      await git.checkout(branch);
      await app.reload();
      result = `Switched to ${branch}.`;
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

  function runSync(kind: SyncActionKind) {
    switch (kind) {
      case "pull":
      case "fetch":
        return pull();
      case "push":
      case "publish":
        return push();
      case "sync":
        return wizard("diverged");
      case "add-remote":
        return wizard("add-remote");
      case "create-branch-here":
        return wizard("detached");
    }
  }

  function openFile(path: string, state: string, e: Event) {
    opener = e.currentTarget as HTMLElement;
    if (state === "conflicted") conflictPath = path;
    else diffPath = path;
  }

  function closeDialog() {
    diffPath = null;
    conflictPath = null;
    const back = opener;
    opener = null;
    void tick().then(() => back?.isConnected && back.focus());
  }

  const PROGRESS: Record<string, string> = {
    push: "Pushing…",
    pull: "Pulling…",
    clone: "Cloning…",
    commit: "Committing…",
    checkout: "Switching…",
    init: "Creating the repository…",
    resolve: "Resolving…",
  };

  const branchItems = $derived.by((): MenuEntry[] => {
    const locked = !!blocked || busy !== null || !!status?.merging;
    const items: MenuEntry[] = [{ heading: "Switch to" }];
    for (const b of localBranches) {
      items.push({
        id: `switch-${b.name}`,
        label: b.name,
        checked: b.name === current,
        disabled: locked,
        run: () => (b.name !== current ? checkout(b.name) : undefined),
      });
    }
    if (remoteOnly.length > 0) {
      items.push({ heading: "On the server only" });
      for (const b of remoteOnly) {
        items.push({ id: `switch-${b.name}`, label: b.name, icon: "pull", disabled: locked, run: () => checkout(b.name) });
      }
    }
    items.push(
      { separator: true },
      { id: "new", label: "New branch…", icon: "add", run: () => wizard("create-branch", { start: "HEAD" }) },
      { id: "merge", label: current ? `Merge into ${current}…` : "Merge into current…", icon: "merge", disabled: !current, run: () => wizard("merge") },
      {
        id: "compare",
        label: "Compare with",
        icon: "compare",
        disabled: compareTargets.length === 0,
        items: compareTargets.map((b) => ({
          id: `compare-${b.name}`,
          label: b.name,
          run: () => views.open({ kind: "diff", scope: { kind: "between", from: current ?? "HEAD", to: b.name } }),
        })),
      },
      { id: "rename", label: "Rename branch…", icon: "edit", disabled: !current, run: () => wizard("rename-branch", { from: current }) },
      { id: "delete", label: "Delete branch…", icon: "delete", danger: true, run: () => wizard("delete-branch") },
    );
    return items;
  });

  const syncItems = $derived.by((): MenuEntry[] => {
    const up = status?.upstream;
    const items: MenuEntry[] = [
      { id: "pull", label: "Pull", icon: "pull", hint: status?.behind ? `↓${status.behind}` : undefined, disabled: !up || !!blocked, run: pull },
      { id: "push", label: up ? "Push" : "Publish branch", icon: "push", hint: status?.ahead ? `↑${status.ahead}` : undefined, disabled: !current, run: push },
      { id: "fetch", label: "Check for changes", icon: "sync", disabled: !up, run: () => wizard("fetch") },
      { id: "remotes", label: "Remotes…", icon: "service", run: () => wizard("remotes") },
    ];
    if (stashes.length > 0) {
      items.push({ id: "unstash", label: "Bring back parked changes…", icon: "stash", hint: String(stashes.length), run: () => wizard("unstash") });
    }
    items.push(
      { separator: true },
      {
        id: "undo",
        label: last ? `Undo: ${last.summary}` : "Undo last Git action",
        icon: "undo",
        disabled: !last || !!last.stale,
        run: () => wizard("undo"),
      },
    );
    return items;
  });

  const changeItems = $derived.by((): MenuEntry[] => [
    { id: "by-field", label: "Show all changes by field", icon: "compare", run: () => views.open({ kind: "diff", scope: { kind: "working" } }) },
    { id: "all", label: selected.length === files.length ? "Uncheck all" : "Check all", icon: "check", run: () => toggleAll(selected.length !== files.length) },
    { separator: true },
    { id: "stash", label: "Park changes…", icon: "stash", disabled: files.length === 0 || conflicted, run: () => wizard("stash") },
    {
      id: "discard",
      label: "Discard changes…",
      icon: "undo",
      danger: true,
      disabled: files.length === 0,
      run: () => wizard("discard", { paths: selected }),
    },
  ]);

  function commitItems(c: GitGraphCommit): MenuEntry[] {
    const onHead = c.oid === head;
    return [
      { id: "show", label: "Show changes", icon: "eye", run: () => views.open({ kind: "diff", scope: { kind: "commit", rev: c.oid } }) },
      { id: "compare", label: "Compare with current", icon: "compare", disabled: onHead, run: () => views.open({ kind: "compare", from: c.oid, to: "HEAD" }) },
      { separator: true },
      { id: "branch", label: "Branch from here…", icon: "branch", run: () => wizard("create-branch", { start: c.oid }) },
      { id: "cherry-pick", label: "Copy this commit here…", icon: "copy", disabled: onHead, run: () => wizard("cherry-pick", { commit: c.oid }) },
      { id: "revert", label: "Undo this commit…", icon: "undo", run: () => wizard("revert", { commit: c.oid }) },
      { id: "tag", label: "Tag…", icon: "tag", run: () => wizard("create-tag", { target: c.oid }) },
      { separator: true },
      { id: "copy", label: "Copy commit id", hint: c.oid.slice(0, 7), run: () => navigator.clipboard?.writeText(c.oid) },
    ];
  }

  /** Icon of the header's main action. */
  const ACTION_ICON: Record<SyncActionKind, string> = {
    pull: "pull",
    push: "push",
    publish: "push",
    sync: "sync",
    fetch: "sync",
    "add-remote": "add",
    "create-branch-here": "branch",
  };

  /** The word a situation's banner explains. */
  const BANNER_TERM: Partial<Record<GitSituation["kind"], GitTerm>> = {
    conflicts: "conflict",
    merging: "merge",
    diverged: "diverged",
    detached: "detached",
  };
</script>

{#snippet explain(term: GitTerm)}
  <div class="explain">
    <Button
      variant="ghost"
      size="sm"
      icon="help"
      data-testid="git-explain-{term}"
      aria-expanded={explaining === term}
      onclick={() => (explaining = explaining === term ? null : term)}>What does this mean?</Button
    >
    {#if explaining === term}{@render gloss(term)}{/if}
  </div>
{/snippet}

{#snippet gloss(term: GitTerm)}
  <p class="gloss" data-testid="git-gloss">
    <strong>{GLOSSARY[term].label}.</strong>
    {GLOSSARY[term].text}
    <span class="git-word">In Git: <code>{GLOSSARY[term].git}</code></span>
  </p>
{/snippet}

{#snippet messages()}
  {#if error}
    <div data-testid="git-error"><Banner tone="danger" dismissible dismissLabel="Dismiss" ondismiss={() => (error = null)}>{error}</Banner></div>
  {/if}
  {#if result}
    <div data-testid="git-result"><Banner tone="ok" dismissible dismissLabel="Dismiss" ondismiss={() => (result = null)}>{result}</Banner></div>
  {/if}
{/snippet}

<div class="git" data-testid="git-panel" data-situation={sit.kind} aria-busy={busy !== null}>
  {#if credentials}
    <form
      data-testid="git-credentials"
      aria-label="Credentials for {credentials.host}"
      onsubmit={(e) => {
        e.preventDefault();
        void saveCredentials();
      }}
    >
      <Card padding="sm" icon="key" title="Sign in to {credentials.host}" description="A username and password, or an access token.">
        <div class="stack">
          <TextField label="Username" data-testid="git-cred-username" autocomplete="username" bind:value={credUser} />
          <TextField label="Password or token" data-testid="git-cred-password" type="password" autocomplete="current-password" bind:value={credPassword} />
          <p class="hint">
            Stored on this device ({app.backend.kind === "web"
              ? "encrypted in this browser"
              : app.backend.kind === "tauri"
                ? "in the system keychain"
                : "in memory"}), never in the collection.
          </p>
        </div>
        {#snippet footer()}
          <Button size="sm" variant="ghost" type="button" data-testid="git-cred-cancel" onclick={() => (credentials = null)}>Cancel</Button>
          <Button size="sm" variant="primary" type="submit" data-testid="git-cred-save" disabled={!credUser || !credPassword}>Save and retry</Button>
        {/snippet}
      </Card>
    </form>
  {/if}

  {#if identityThen}
    <form
      data-testid="git-identity"
      aria-label="Author for commits"
      novalidate
      onsubmit={(e) => {
        e.preventDefault();
        void saveIdentity();
      }}
    >
      <Card padding="sm" icon="user" title="Who is committing?" description="Name and email go into every commit.">
        <div class="stack">
          <TextField label="Name" data-testid="git-identity-name" autocomplete="name" bind:value={identityName} />
          <TextField label="Email" data-testid="git-identity-email" type="email" autocomplete="email" bind:value={identityEmail} />
        </div>
        {#snippet footer()}
          <Button size="sm" variant="ghost" type="button" onclick={() => (identityThen = null)}>Cancel</Button>
          <Button size="sm" variant="primary" type="submit" data-testid="git-identity-save">Save</Button>
        {/snippet}
      </Card>
    </form>
  {/if}

  {#if loadError}
    <div data-testid="git-load-error"><Banner tone="danger" title="Cannot read the repository">{loadError}</Banner></div>
  {:else if sit.kind === "loading"}
    <div class="loading"><Spinner size="sm" /> <span class="hint">Reading the repository…</span></div>
  {:else if sit.kind === "no-repo"}
    <div class="empty" data-testid="git-norepo">
      <EmptyState
        icon="history"
        title="No version history yet"
        text="Keep every version of your requests in Git: start a repository here, or clone one your team already has."
        size="sm"
      >
        {#snippet actions()}
          <Button variant="primary" size="sm" icon="add" data-testid="git-init" disabled={busy !== null} loading={busy === "init"} onclick={init}>Start a repository here</Button>
          {#if !cloneOpen}
            <Button size="sm" icon="pull" data-testid="git-clone-open" onclick={() => (cloneOpen = true)}>Clone…</Button>
          {/if}
        {/snippet}
      </EmptyState>
      {#if app.backend.kind === "web"}
        <p class="hint center">In the browser, the repository must be the collection folder itself: the app cannot see the folders above it.</p>
      {/if}
    </div>
  {:else}
    <div class="repo" data-testid="git-repo">
      <Card padding="sm">
        <div class="repo-line">
          <span class="repo-icon" aria-hidden="true"><Icon name="collection" size="sm" /></span>
          <span class="repo-name" data-testid="git-repo-name" use:tooltip={`${GLOSSARY.repository.label}: ${GLOSSARY.repository.text}`}>{name}</span>
          <IconButton
            icon="help"
            size="sm"
            label="What do these words mean?"
            data-testid="git-help"
            aria-expanded={explaining !== null}
            onclick={() => (explaining = explaining ? null : (BANNER_TERM[sit.kind] ?? sync?.term ?? "branch"))}
          />
          <Dropdown items={syncItems} menuLabel="Sync and more" placement="bottom-end">
            {#snippet trigger(p)}<IconButton {...p} icon="more" size="sm" label="Sync and more" data-testid="git-sync-menu" />{/snippet}
          </Dropdown>
        </div>
        <div class="branch-line">
          <span class="branch-chip">
            <Dropdown items={branchItems} menuLabel="Branch {current ?? ''}">
              {#snippet trigger(p)}
                <Chip
                  {...p}
                  icon="branch"
                  mono
                  caret
                  tone={sit.kind === "detached" ? "warn" : undefined}
                  data-testid="git-branch"
                  title="{GLOSSARY['current-branch'].label}: {GLOSSARY['current-branch'].text}"
                  label={sit.kind === "detached" ? (head?.slice(0, 7) ?? "HEAD") : sit.kind === "empty" ? `${sit.branch} (no commits yet)` : (current ?? "")}
                />
              {/snippet}
            </Dropdown>
          </span>
          {#if sync}
            <span class="sync {sync.tone}" data-testid="git-sync" data-ahead={sync.ahead} data-behind={sync.behind} aria-label="{sync.title} {GLOSSARY[sync.term].text}" use:tooltip={`${sync.title} ${GLOSSARY[sync.term].text}`}>
              {#if sync.ahead || sync.behind}
                {#if sync.ahead}<span class="count"><Icon name="push" size="sm" />{sync.ahead}</span>{/if}
                {#if sync.behind}<span class="count"><Icon name="pull" size="sm" />{sync.behind}</span>{/if}
                {#if status?.upstream}<span class="upstream">{status.upstream}</span>{/if}
              {:else}
                {#if sync.tone === "ok"}<Icon name="success" size="sm" />{/if}
                <span class="sync-text">{sync.text}</span>
              {/if}
            </span>
          {/if}
        </div>
        {#if primary && !BANNER_TERM[sit.kind]}
          <div class="sync-action" data-action={primary.kind}>
            <Button
              block
              size="md"
              variant={primary.primary ? "primary" : "secondary"}
              icon={ACTION_ICON[primary.kind]}
              data-testid="git-sync-action"
              data-action={primary.kind}
              title={primary.title}
              loading={busy === "pull" || busy === "push"}
              disabled={busy !== null || ((primary.kind === "pull" || primary.kind === "fetch") && !!blocked)}
              onclick={() => runSync(primary.kind)}>{primary.label}</Button
            >
          </div>
        {/if}
        {#if busy === "pull" || busy === "push" || busy === "fetch"}
          <Progress size="sm" label={PROGRESS[busy] ?? "Working…"} data-testid="git-progress-bar" />
        {/if}
        {#if siblings.length > 0}
          <p class="hint" data-testid="git-siblings">Also in this repository: {siblings.map((c) => c.name).join(", ")}</p>
        {/if}
        {#if explaining && !BANNER_TERM[sit.kind]}{@render gloss(explaining)}{/if}
      </Card>
    </div>

    {#if blocked}
      <div data-testid="git-blocked"><Banner tone="warn">{blocked}</Banner></div>
    {/if}
    {@render messages()}
    {#if busy && PROGRESS[busy] && !primary}
      <div class="loading" role="status" data-testid="git-progress"><Spinner size="sm" /> <span class="hint">{PROGRESS[busy]}</span></div>
    {/if}

    {#if sit.kind === "conflicts"}
      <div data-testid="git-merging">
        <Banner tone="danger" title="The merge needs your decision">
          The same {sit.files === 1 ? "request was" : "requests were"} changed here and on the server. Choose which version to keep, then commit.
          <span class="banner-actions">
            <Button size="sm" variant="primary" icon="merge" data-testid="git-resolve" onclick={() => views.open({ kind: "merge" })}>Resolve conflicts</Button>
          </span>
          {@render explain("conflict")}
        </Banner>
      </div>
    {:else if sit.kind === "merging"}
      <div data-testid="git-merging">
        <Banner tone="info" title="Almost done">
          Every conflict is resolved. Commit to finish the merge.
          {@render explain("merge")}
        </Banner>
      </div>
    {:else if sit.kind === "diverged"}
      <div data-testid="git-diverged">
        <Banner tone="warn" title="{sit.branch} and {sit.upstream} went apart">
          You have {sit.ahead} new {sit.ahead === 1 ? "commit" : "commits"}, the server has {sit.behind}. Bring them together before you push.
          <span class="banner-actions">
            <Button size="sm" variant="primary" icon="merge" data-testid="git-diverged-merge" disabled={!!blocked || busy !== null} loading={busy === "pull"} onclick={pull}>Merge both</Button>
            <Button size="sm" data-testid="git-diverged-theirs" onclick={() => wizard("diverged", { choice: "theirs" })}>Take theirs…</Button>
          </span>
          {@render explain("diverged")}
        </Banner>
      </div>
    {:else if sit.kind === "detached"}
      {@const back = localBranches.find((b) => b.name === "main")?.name ?? localBranches[0]?.name}
      <div data-testid="git-detached">
        <Banner tone="warn" title="You are looking at an older commit">
          New commits here belong to no branch. Create a branch to keep them, or go back to a branch.
          <span class="banner-actions">
            <Button size="sm" variant="primary" icon="branch" data-testid="git-detached-branch" onclick={() => wizard("create-branch", { start: "HEAD" })}>Create a branch here</Button>
            {#if back}
              <Button size="sm" data-testid="git-detached-back" disabled={!!blocked || busy !== null} onclick={() => checkout(back)}>Back to {back}</Button>
            {/if}
          </span>
          {@render explain("detached")}
        </Banner>
      </div>
    {/if}

    <div class="changes" data-testid="git-changes">
      <Section title="Changes" count={files.length > 0 ? files.length : undefined}>
        {#snippet actions()}
          {#if files.length > 0}
            <IconButton icon="compare" size="sm" label="Show all changes by field" data-testid="git-semantic-diff" onclick={() => views.open({ kind: "diff", scope: { kind: "working" } })} />
          {/if}
          <Dropdown items={changeItems} menuLabel="More for changes" placement="bottom-end">
            {#snippet trigger(p)}<IconButton {...p} icon="more" size="sm" label="More for changes" data-testid="git-changes-menu" />{/snippet}
          </Dropdown>
        {/snippet}
        {#if files.length === 0}
          <p class="clean" data-testid="git-clean">
            <Icon name="success" size="sm" />
            {sit.kind === "empty" ? "Nothing to commit yet." : "No changes. Everything is committed."}
          </p>
        {:else}
          {#if sit.kind === "empty"}<p class="hint">Commit these files to start the history.</p>{/if}
          <ul class="files">
            {#each rows as f (f.path)}
              <li data-testid="git-file" data-path={f.path}>
                <Checkbox
                  data-testid="git-file-check"
                  aria-label="Include {f.path}"
                  checked={!excluded.includes(f.path)}
                  onchange={(e: Event) => toggle(f.path, (e.currentTarget as HTMLInputElement).checked)}
                />
                <button class="row" data-testid="git-file-open" title="{f.path}: {f.badgeTitle}" onclick={(e) => openFile(f.path, f.state, e)}>
                  {#if f.method}<Badge kind="method" value={f.method} />{/if}
                  <span class="file-title">{f.title}</span>
                  {#if f.dir}<span class="file-dir">{f.dir}</span>{/if}
                </button>
                <Badge kind="status" value={f.badge} title={f.badgeTitle} data-testid="git-file-state" />
              </li>
            {/each}
          </ul>
        {/if}
        {#if files.length > 0 || status?.merging}
          <form
            class="commit"
            onsubmit={(e) => {
              e.preventDefault();
              if (canCommit) void commit();
            }}
          >
            <TextArea
              data-testid="git-message"
              aria-label="Commit message"
              placeholder="Describe what changed"
              rows={2}
              bind:value={message}
              onkeydown={(e: KeyboardEvent) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && canCommit) {
                  e.preventDefault();
                  void commit();
                }
              }}
            />
            <div class="commit-button">
              <SplitButton
                variant="primary"
                icon="commit"
                data-testid="git-commit"
                disabled={!canCommit}
                loading={busy === "commit"}
                menuLabel="More commit options"
                items={[
                  {
                    id: "push",
                    label: status?.upstream ? "Commit & push" : "Commit & publish",
                    icon: "push",
                    disabled: !canCommit || !current,
                    run: () => commit(true),
                  },
                ]}
                onclick={() => canCommit && void commit()}>{commitLabel(selected.length)}</SplitButton
              >
            </div>
          </form>
          {#if findings.length > 0}<SecretFindings {findings} />{/if}
        {/if}
      </Section>
    </div>

    {#if tiles.length > 0}<ToolsGrid {tiles} />{/if}

    {#if recent.length > 0}
      <RecentCommits
        commits={recent}
        {refs}
        {head}
        onshow={(oid) => views.open({ kind: "diff", scope: { kind: "commit", rev: oid } })}
        actions={commitItems}
        ongraph={() => views.open({ kind: "graph" })}
      />
    {/if}
  {/if}

  {#if status && cloneOpen}
    <form
      class="clone"
      aria-label="Clone a repository"
      onsubmit={(e) => {
        e.preventDefault();
        if (cloneUrl.trim()) void clone();
      }}
    >
      <Card padding="sm" icon="pull" title="Clone a repository" description={app.backend.kind === "web" ? "Opens the clone as the collection; pick the folder to clone into." : "Opens the clone as the collection."}>
        <CloneFields bind:url={cloneUrl} disabled={busy !== null} />
        {#if busy === "clone"}<Progress size="sm" label="Cloning…" />{/if}
        {#snippet footer()}
          <Button size="sm" variant="ghost" type="button" onclick={() => (cloneOpen = false)}>Cancel</Button>
          <Button size="sm" variant="primary" type="submit" data-testid="git-clone" disabled={!cloneUrl.trim() || busy !== null} loading={busy === "clone"}>Clone</Button>
        {/snippet}
      </Card>
    </form>
  {:else if status?.repo}
    <div class="footer">
      <Button variant="ghost" size="sm" icon="pull" data-testid="git-clone-open" onclick={() => (cloneOpen = true)}>Clone another repository…</Button>
    </div>
  {/if}
  {#if status && !status.repo}{@render messages()}{/if}
</div>

{#if diffPath}
  <DiffView path={diffPath} onclose={closeDialog} />
{/if}
{#if conflictPath}
  <ConflictView path={conflictPath} {blocked} onresolve={(r) => conflictPath && resolve(conflictPath, r)} onclose={closeDialog} />
{/if}

<style>
  .git {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
    box-sizing: border-box;
    height: 100%;
    overflow: auto;
    padding: var(--rt-space-4) var(--rt-side-inset) var(--rt-space-6);
    font: var(--rt-font-body);
  }
  p {
    margin: 0;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
  }
  .hint {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .center {
    text-align: center;
  }
  .loading {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
  }
  .empty {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
    padding-top: var(--rt-space-7);
  }

  /* Repository card */
  .repo {
    container-type: inline-size;
  }
  .repo-line,
  .branch-line {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    min-width: 0;
  }
  .repo-icon {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: var(--rt-control-sm);
    height: var(--rt-control-sm);
    border-radius: var(--rt-radius-md);
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .repo-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--rt-font-title);
    letter-spacing: var(--rt-tracking-tight);
  }
  .branch-chip {
    display: flex;
    flex: 0 1 auto;
    min-width: 5em;
    max-width: 100%;
  }
  .branch-chip :global(.rt-dropdown),
  .branch-chip :global(.rt-chip),
  .branch-chip :global(.rt-chip .hit) {
    min-width: 0;
    max-width: 100%;
  }
  .branch-chip :global(.rt-chip .label) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sync {
    display: inline-flex;
    flex: 0 1000 auto;
    align-items: center;
    gap: var(--rt-space-3);
    min-width: 0;
    overflow: hidden;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    white-space: nowrap;
  }
  .sync.ok {
    color: var(--rt-ok-fg);
  }
  .sync.attention {
    color: var(--rt-accent);
  }
  .sync.warn {
    color: var(--rt-warn-fg);
  }
  .count {
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-1);
    font-weight: var(--rt-weight-semibold);
    font-variant-numeric: tabular-nums;
  }
  .sync-text,
  .upstream {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .upstream {
    color: var(--rt-fg-muted);
    font-family: var(--rt-font-mono);
  }
  @container (max-width: 220px) {
    .upstream {
      display: none;
    }
  }
  .gloss {
    padding: var(--rt-space-4) var(--rt-space-5);
    border-radius: var(--rt-radius-md);
    background: var(--rt-surface-sunken);
    color: var(--rt-fg);
    font: var(--rt-font-small);
  }
  .git-word {
    display: block;
    margin-top: var(--rt-space-2);
    color: var(--rt-fg-muted);
  }
  .banner-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-3);
    margin-top: var(--rt-space-4);
  }
  .explain {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--rt-space-3);
    margin-top: var(--rt-space-3);
  }
  .banner-actions :global(.rt-btn),
  .explain :global(.rt-btn) {
    min-width: 0;
    max-width: 100%;
  }

  /* Changes */
  .clean {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .files {
    display: flex;
    flex-direction: column;
    margin: 0 calc(-1 * var(--rt-space-3));
    padding: 0;
    list-style: none;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    min-height: var(--rt-control-md);
    padding: 0 var(--rt-space-3);
    border-radius: var(--rt-radius-md);
  }
  .files li:hover {
    background: var(--rt-bg-hover);
  }
  /* A list row: the row is the control, drawn with the row tokens (README: rows use
     --rt-control-md and --rt-bg-hover). */
  .row {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--rt-space-3);
    min-width: 0;
    height: var(--rt-control-md);
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--rt-fg);
    font: var(--rt-font-body);
    text-align: left;
    cursor: pointer;
  }
  .row:focus-visible {
    outline: var(--rt-focus-width) solid var(--rt-focus);
    outline-offset: calc(-1 * var(--rt-focus-width));
    border-radius: var(--rt-radius-sm);
  }
  .file-title {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file-dir {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--rt-fg-subtle);
    font: var(--rt-font-small);
  }
  .commit {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
    margin-top: var(--rt-space-4);
  }
  .commit-button {
    display: flex;
    min-width: 0;
  }
  .commit-button :global(.rt-split) {
    flex: 1;
    min-width: 0;
  }
  .commit-button :global(.rt-split .main) {
    flex: 1;
    min-width: 0;
  }
  .footer {
    margin-top: auto;
  }
</style>

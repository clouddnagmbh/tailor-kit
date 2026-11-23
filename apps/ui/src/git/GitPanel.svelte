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
  import MethodBadge from "../shell/MethodBadge.svelte";
  import { getApp } from "../state/app.svelte";
  import { gitApiCommands } from "./api-commands";
  import CloneFields from "./CloneFields.svelte";
  import ConflictView from "./ConflictView.svelte";
  import DiffView from "./DiffView.svelte";
  import { commitMessage, mergeMessage, pullMessage, pushMessage } from "./format";
  import { GLOSSARY, type GitTerm } from "./glossary";
  import Menu, { type MenuItem } from "./panel/Menu.svelte";
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
  import { getGitViews } from "./views.svelte";
  import { openGitWizard, type GitWizardKind } from "./wizards/open";

  let { onopen }: { onopen: (info: CollectionInfo) => Promise<void> } = $props();
  const app = getApp();
  const views = getGitViews();
  const commands = $derived(gitApiCommands(app, views));
  const git = app.backend.git;
  const RECENT = 8;

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
    const done: GitActionResult | undefined = await openGitWizard(app, { kind, preset });
    if (done) {
      if (done.reload) await app.reload();
      result = done.message;
    }
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

  const branchItems = $derived.by((): MenuItem[] => {
    const items: MenuItem[] = [{ id: "h-local", label: "Switch to", heading: true }];
    for (const b of localBranches) {
      items.push({
        id: `switch-${b.name}`,
        label: b.name,
        checked: b.name === current,
        disabled: !!blocked || busy !== null || !!status?.merging,
        title: blocked ?? undefined,
        run: () => (b.name !== current ? checkout(b.name) : undefined),
      });
    }
    if (remoteOnly.length > 0) {
      items.push({ id: "h-remote", label: "On the server only", heading: true });
      for (const b of remoteOnly) {
        items.push({
          id: `switch-${b.name}`,
          label: b.name,
          checked: false,
          disabled: !!blocked || busy !== null || !!status?.merging,
          title: "Creates a local branch linked to it and switches to it.",
          run: () => checkout(b.name),
        });
      }
    }
    items.push(
      { id: "sep-1", label: "", separator: true },
      { id: "new", label: "New branch…", run: () => wizard("create-branch", { start: "HEAD" }) },
      {
        id: "merge",
        label: current ? `Merge into ${current}…` : "Merge into current…",
        disabled: !current,
        title: GLOSSARY.merge.text,
        run: () => wizard("merge"),
      },
      { id: "rename", label: "Rename branch…", disabled: !current, run: () => wizard("rename-branch", { from: current }) },
      { id: "delete", label: "Delete branch…", danger: true, run: () => wizard("delete-branch") },
    );
    return items;
  });

  const syncItems = $derived.by((): MenuItem[] => {
    const up = status?.upstream;
    const items: MenuItem[] = [
      { id: "pull", label: "Pull", hint: status?.behind ? `↓${status.behind}` : undefined, title: GLOSSARY.pull.text, disabled: !up || !!blocked, run: pull },
      { id: "push", label: up ? "Push" : "Publish branch", hint: status?.ahead ? `↑${status.ahead}` : undefined, title: up ? GLOSSARY.push.text : GLOSSARY.publish.text, disabled: !current, run: push },
      { id: "fetch", label: "Check for changes", title: GLOSSARY.fetch.text, disabled: !up, run: () => wizard("fetch") },
      { id: "remotes", label: "Remotes…", title: GLOSSARY.remote.text, run: () => wizard("remotes") },
    ];
    if (stashes.length > 0) {
      items.push({ id: "unstash", label: "Bring back parked changes…", hint: String(stashes.length), run: () => wizard("unstash") });
    }
    items.push(
      { id: "sep-u", label: "", separator: true },
      {
        id: "undo",
        label: last ? `Undo: ${last.summary}` : "Undo last Git action",
        disabled: !last || !!last.stale,
        title: last?.stale ?? GLOSSARY.backup.text,
        run: () => wizard("undo"),
      },
    );
    return items;
  });

  const changeItems = $derived.by((): MenuItem[] => [
    { id: "by-field", label: "Show all changes by field", run: () => views.open({ kind: "diff", scope: { kind: "working" } }) },
    { id: "all", label: selected.length === files.length ? "Uncheck all" : "Check all", run: () => toggleAll(selected.length !== files.length) },
    { id: "sep", label: "", separator: true },
    { id: "stash", label: "Park changes…", title: GLOSSARY.stash.text, disabled: files.length === 0 || conflicted, run: () => wizard("stash") },
    {
      id: "discard",
      label: "Discard changes…",
      danger: true,
      disabled: files.length === 0,
      title: "Throws away the changes of the checked files; the app keeps a backup.",
      run: () => wizard("discard", { paths: selected.map((p) => `/${p}`) }),
    },
  ]);

  function commitItems(c: GitGraphCommit): MenuItem[] {
    const onHead = c.oid === head;
    return [
      { id: "show", label: "Show changes", run: () => views.open({ kind: "diff", scope: { kind: "commit", rev: c.oid } }) },
      { id: "compare", label: "Compare with current", disabled: onHead, run: () => views.open({ kind: "compare", from: c.oid, to: "HEAD" }) },
      { id: "sep-1", label: "", separator: true },
      { id: "branch", label: "Branch from here…", title: GLOSSARY.branch.text, run: () => wizard("create-branch", { start: c.oid }) },
      { id: "cherry-pick", label: "Copy this commit here…", title: GLOSSARY["cherry-pick"].text, disabled: onHead, run: () => wizard("cherry-pick", { commit: c.oid }) },
      { id: "revert", label: "Undo this commit…", title: GLOSSARY.revert.text, run: () => wizard("revert", { commit: c.oid }) },
      { id: "tag", label: "Tag…", title: GLOSSARY.tag.text, run: () => wizard("create-tag", { target: c.oid }) },
      { id: "sep-2", label: "", separator: true },
      { id: "copy", label: "Copy commit id", hint: c.oid.slice(0, 7), run: () => navigator.clipboard?.writeText(c.oid) },
    ];
  }

  /** The word a situation's banner explains. */
  const BANNER_TERM: Partial<Record<GitSituation["kind"], GitTerm>> = {
    conflicts: "conflict",
    merging: "merge",
    diverged: "diverged",
    detached: "detached",
  };
</script>

{#snippet explain(term: GitTerm)}
  <button
    class="link explain"
    data-testid="git-explain-{term}"
    aria-expanded={explaining === term}
    onclick={() => (explaining = explaining === term ? null : term)}>What does this mean?</button
  >
  {#if explaining === term}
    <p class="gloss" data-testid="git-gloss">
      <strong>{GLOSSARY[term].label}</strong>
      {GLOSSARY[term].text}
      <span class="git-word">In Git: {GLOSSARY[term].git}</span>
    </p>
  {/if}
{/snippet}

<div class="git" data-testid="git-panel" data-situation={sit.kind} aria-busy={busy !== null}>
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

  {#if loadError}
    <p class="error" role="alert" data-testid="git-load-error">{loadError}</p>
  {:else if sit.kind === "loading"}
    <p class="hint">Reading the repository…</p>
  {:else if sit.kind === "no-repo"}
    <section class="empty" data-testid="git-norepo">
      <h3 class="empty-title">No version history yet</h3>
      <p class="hint">
        This collection is not in a Git repository. Start one to keep every version of your requests, or clone an
        existing one.
      </p>
      {#if app.backend.kind === "web"}
        <p class="hint">In the browser, the repository must be the collection folder itself: the app cannot see the folders above it.</p>
      {/if}
      <button class="primary wide" data-testid="git-init" disabled={busy !== null} onclick={init}>Start a repository here</button>
    </section>
  {:else}
    <section class="repo card" aria-label="Repository" data-testid="git-repo">
      <div class="repo-line">
        <span class="repo-name" data-testid="git-repo-name" title="{GLOSSARY.repository.label}: {GLOSSARY.repository.text}">{name}</span>
        <Menu items={syncItems} label="Sync and more" testid="git-sync-menu" align="end" triggerClass="icon-button" title="Pull, push, remotes, undo">
          {#snippet trigger()}<span aria-hidden="true">⋯</span>{/snippet}
        </Menu>
        <button
          class="help"
          data-testid="git-help"
          aria-label="What do these words mean?"
          aria-expanded={explaining !== null}
          title="What do these words mean?"
          onclick={() => (explaining = explaining ? null : (BANNER_TERM[sit.kind] ?? sync?.term ?? "branch"))}>?</button
        >
      </div>
      <div class="branch-line">
        <Menu
          items={branchItems}
          label="Branch {current ?? ''}: switch, create, merge"
          testid="git-branch-menu"
          triggerClass="chip"
          title="{GLOSSARY['current-branch'].label}: {GLOSSARY['current-branch'].text}"
        >
          {#snippet trigger()}
            <span class="chip-icon" aria-hidden="true">⑂</span>
            <span class="chip-name" data-testid="git-branch">{sit.kind === "detached" ? (head?.slice(0, 7) ?? "HEAD") : sit.kind === "empty" ? `${sit.branch} (no commits yet)` : (current ?? "")}</span>
            <span class="caret" aria-hidden="true">▾</span>
          {/snippet}
        </Menu>
        {#if sync}
          <span class="sync {sync.tone}" data-testid="git-sync" title="{sync.title} {GLOSSARY[sync.term].text}">
            {sync.text}{#if status?.upstream && (sync.ahead || sync.behind)}<span class="upstream">{status.upstream}</span>{/if}
          </span>
        {/if}
      </div>
      {#if primary && !BANNER_TERM[sit.kind]}
        <button
          class="sync-action"
          class:primary={primary.primary}
          data-testid="git-sync-action"
          data-action={primary.kind}
          title={primary.title}
          disabled={busy !== null || ((primary.kind === "pull" || primary.kind === "fetch") && !!blocked)}
          onclick={() => runSync(primary.kind)}>{busy && PROGRESS[busy] ? PROGRESS[busy] : primary.label}</button
        >
      {/if}
      {#if siblings.length > 0}
        <p class="hint" data-testid="git-siblings">Also in this repository: {siblings.map((c) => c.name).join(", ")}</p>
      {/if}
      {#if explaining && !BANNER_TERM[sit.kind]}
        <p class="gloss" data-testid="git-gloss">
          <strong>{GLOSSARY[explaining].label}</strong>
          {GLOSSARY[explaining].text}
          <span class="git-word">In Git: {GLOSSARY[explaining].git}</span>
        </p>
      {/if}
    </section>

    {#if blocked}<p class="warn" data-testid="git-blocked">{blocked}</p>{/if}
    {#if error}<p class="error" role="alert" data-testid="git-error">{error}</p>{/if}
    {#if result}<p class="result" role="status" data-testid="git-result">{result}</p>{/if}
    {#if busy && PROGRESS[busy] && !primary}<p class="progress" role="status" data-testid="git-progress">{PROGRESS[busy]}</p>{/if}

    {#if sit.kind === "conflicts"}
      <section class="banner danger" data-testid="git-merging" aria-label="Merge stopped">
        <strong>The merge needs your decision</strong>
        <p>The same {sit.files === 1 ? "request was" : "requests were"} changed here and on the server. Choose which version to keep, then commit.</p>
        <div class="row">
          <button class="primary" data-testid="git-resolve" onclick={() => views.open({ kind: "merge" })}>Resolve conflicts</button>
        </div>
        {@render explain("conflict")}
      </section>
    {:else if sit.kind === "merging"}
      <section class="banner" data-testid="git-merging" aria-label="Merge in progress">
        <strong>Almost done</strong>
        <p>Every conflict is resolved. Commit to finish the merge.</p>
        {@render explain("merge")}
      </section>
    {:else if sit.kind === "diverged"}
      <section class="banner warn" data-testid="git-diverged" aria-label="Branches went apart">
        <strong>{sit.branch} and {sit.upstream} went apart</strong>
        <p>You have {sit.ahead} new {sit.ahead === 1 ? "commit" : "commits"}, the server has {sit.behind}. Bring them together before you push.</p>
        <div class="row">
          <button class="primary" data-testid="git-diverged-merge" disabled={!!blocked || busy !== null} onclick={pull}>Merge both</button>
          <button data-testid="git-diverged-theirs" onclick={() => wizard("diverged", { choice: "theirs" })}>Take theirs…</button>
        </div>
        {@render explain("diverged")}
      </section>
    {:else if sit.kind === "detached"}
      <section class="banner warn" data-testid="git-detached" aria-label="Not on a branch">
        <strong>You are looking at an older commit</strong>
        <p>New commits here belong to no branch. Create a branch to keep them, or go back to a branch.</p>
        <div class="row">
          <button class="primary" data-testid="git-detached-branch" onclick={() => wizard("create-branch", { start: "HEAD" })}>Create a branch here</button>
          {#if localBranches.length > 0}
            <button data-testid="git-detached-back" disabled={!!blocked || busy !== null} onclick={() => checkout(localBranches.find((b) => b.name === "main")?.name ?? localBranches[0].name)}
              >Back to {localBranches.find((b) => b.name === "main")?.name ?? localBranches[0].name}</button
            >
          {/if}
        </div>
        {@render explain("detached")}
      </section>
    {/if}

    <section class="changes" aria-label="Changes">
      <header class="section-head">
        <h3>Changes</h3>
        {#if files.length > 0}<span class="count" data-testid="git-change-count">{files.length}</span>{/if}
        <span class="spacer"></span>
        {#if files.length > 0}
          <button class="link" data-testid="git-semantic-diff" title="Every change, field by field" onclick={() => views.open({ kind: "diff", scope: { kind: "working" } })}>By field</button>
        {/if}
        <Menu items={changeItems} label="More for changes" testid="git-changes-menu" align="end" triggerClass="icon-button" title="More">
          {#snippet trigger()}<span aria-hidden="true">⋯</span>{/snippet}
        </Menu>
      </header>
      {#if files.length === 0}
        <p class="hint clean" data-testid="git-clean">
          {sit.kind === "empty" ? "Nothing to commit yet." : "No changes. Everything is committed."}
        </p>
      {:else}
        {#if sit.kind === "empty"}<p class="hint">Commit these files to start the history.</p>{/if}
        <ul class="files">
          {#each rows as f (f.path)}
            <li data-testid="git-file" data-path={f.path}>
              <input
                type="checkbox"
                data-testid="git-file-check"
                aria-label="Include {f.path}"
                checked={!excluded.includes(f.path)}
                onchange={(e) => toggle(f.path, e.currentTarget.checked)}
              />
              <button class="file" data-testid="git-file-open" title="{f.path}: {f.badgeTitle}" onclick={(e) => openFile(f.path, f.state, e)}>
                {#if f.method}<MethodBadge method={f.method} />{/if}
                <span class="file-title">{f.title}</span>
                {#if f.dir}<span class="file-dir">{f.dir}</span>{/if}
              </button>
              <span class="badge {f.state}" data-testid="git-file-state" title={f.badgeTitle} aria-label={f.badgeTitle}>{f.badge}</span>
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
          <textarea
            data-testid="git-message"
            aria-label="Commit message"
            placeholder="Describe what changed"
            rows="2"
            bind:value={message}
            onkeydown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && canCommit) {
                e.preventDefault();
                void commit();
              }
            }}
          ></textarea>
          <div class="split">
            <button class="main primary" data-testid="git-commit" type="submit" disabled={!canCommit}>
              {busy === "commit" ? "Committing…" : commitLabel(selected.length)}
            </button>
            <Menu
              items={[
                {
                  id: "push",
                  label: status?.upstream ? "Commit & push" : "Commit & publish",
                  disabled: !canCommit || !current,
                  title: "Commits, then sends the commit to the server.",
                  run: () => commit(true),
                },
              ]}
              label="More commit options"
              testid="git-commit-more"
              align="end"
              triggerClass="side primary"
              disabled={!canCommit}
            >
              {#snippet trigger()}<span aria-hidden="true">▾</span>{/snippet}
            </Menu>
          </div>
        </form>
        {#if findings.length > 0}<SecretFindings {findings} />{/if}
      {/if}
    </section>

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

  {#if status && (!status.repo || cloneOpen)}
    <form
      class="card clone"
      aria-label="Clone a repository"
      onsubmit={(e) => {
        e.preventDefault();
        if (cloneUrl.trim()) void clone();
      }}
    >
      <CloneFields bind:url={cloneUrl} disabled={busy !== null} />
      {#if app.backend.kind === "web"}<p class="hint">Opens the clone as the collection; pick the folder to clone into.</p>{/if}
      <button data-testid="git-clone" type="submit" disabled={!cloneUrl.trim() || busy !== null}>
        {busy === "clone" ? "Cloning…" : "Clone"}
      </button>
    </form>
  {:else if status}
    <button class="link footer" data-testid="git-clone-open" onclick={() => (cloneOpen = true)}>Clone another repository…</button>
  {/if}
  {#if !status?.repo}
    {#if error}<p class="error" role="alert" data-testid="git-error">{error}</p>{/if}
    {#if result}<p class="result" role="status" data-testid="git-result">{result}</p>{/if}
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
    padding: var(--rt-gap) var(--rt-side-inset);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 12px;
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
  textarea {
    min-width: 0;
    box-sizing: border-box;
    padding: 4px 6px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font: inherit;
  }
  textarea {
    width: 100%;
    resize: vertical;
  }
  button,
  .git :global(.chip),
  .git :global(.side),
  .git :global(.icon-button) {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font: inherit;
    cursor: pointer;
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    background: var(--rt-bg-hover);
  }
  button:disabled {
    color: var(--rt-fg-muted);
    cursor: default;
    opacity: 0.6;
  }
  button.primary:not(:disabled),
  .git :global(.side.primary:not(:disabled)) {
    background: var(--rt-accent);
    border-color: var(--rt-accent);
    color: var(--rt-accent-fg);
  }
  .git :global(.link) {
    height: auto;
    border: none;
    background: none;
    padding: 0;
    color: var(--rt-accent);
    font-size: var(--rt-size-small);
  }
  .git :global(.link:hover:not(:disabled)) {
    background: none;
    text-decoration: underline;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .hint {
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .repo-line,
  .branch-line {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .repo-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .help {
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 50%;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  .branch-line :global(.menu-root) {
    flex: 0 1 auto;
    min-width: 4em;
  }
  .git :global(.chip) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    max-width: 100%;
    padding: 0 8px;
    border-radius: 13px;
  }
  .chip-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 600;
  }
  .chip-icon,
  .caret {
    flex: none;
    color: var(--rt-fg-muted);
  }
  .sync {
    flex: 0 1000 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .sync.ok {
    color: var(--rt-ok);
  }
  .sync.attention {
    color: var(--rt-accent);
  }
  .sync.warn {
    color: var(--rt-warn);
  }
  .upstream {
    margin-left: 6px;
    color: var(--rt-fg-muted);
  }
  .sync-action {
    width: 100%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .repo {
    container-type: inline-size;
  }
  @container (max-width: 220px) {
    .upstream {
      display: none;
    }
  }
  .split {
    display: flex;
    min-width: 0;
  }
  .split .main {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .split :global(.side) {
    width: 26px;
    padding: 0;
    border-left: none;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
  }
  .split :global(.side.primary:not(:disabled)) {
    border-left: 1px solid color-mix(in srgb, var(--rt-accent-fg) 35%, transparent);
  }
  .banner {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: var(--rt-gap);
    border: 1px solid var(--rt-border);
    border-left: 3px solid var(--rt-accent);
    border-radius: var(--rt-radius);
    background: var(--rt-bg-panel);
    font-size: var(--rt-size-small);
  }
  .banner.warn {
    border-left-color: var(--rt-warn);
  }
  .banner.danger {
    border-left-color: var(--rt-danger);
  }
  .explain {
    align-self: flex-start;
  }
  .gloss {
    color: var(--rt-fg);
    font-size: var(--rt-size-small);
  }
  .git-word {
    display: block;
    color: var(--rt-fg-muted);
  }
  .git :global(.section-head) {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 24px;
    margin-bottom: 4px;
  }
  .git :global(.section-head h3) {
    margin: 0;
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .git :global(.section-head .link) {
    margin-left: auto;
  }
  .spacer {
    flex: 1;
  }
  .count {
    padding: 0 6px;
    border-radius: 8px;
    background: var(--rt-bg-hover);
    color: var(--rt-fg-muted);
    font-size: 11px;
    line-height: 16px;
  }
  .git :global(.icon-button) {
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: none;
    color: var(--rt-fg-muted);
  }
  .files {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 26px;
    border-radius: var(--rt-radius);
  }
  .files li:hover {
    background: var(--rt-bg-hover);
  }
  .file {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
    height: auto;
    padding: 3px 0;
    border: none;
    background: none;
    text-align: left;
  }
  .file:hover:not(:disabled) {
    background: none;
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
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .badge {
    flex: none;
    width: 16px;
    margin-right: 4px;
    text-align: center;
    font: 700 11px / 16px var(--rt-font-mono);
    border-radius: 3px;
  }
  .badge.untracked {
    color: var(--rt-ok);
  }
  .badge.modified {
    color: var(--rt-warn);
  }
  .badge.deleted,
  .badge.conflicted {
    color: var(--rt-danger);
  }
  .commit {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 16px 4px;
    text-align: center;
  }
  .empty-title {
    margin: 0;
    font-size: var(--rt-size);
  }
  .wide {
    align-self: stretch;
  }
  .footer {
    align-self: flex-start;
    margin-top: auto;
  }
  .error {
    color: var(--rt-danger);
    font-size: var(--rt-size-small);
  }
  .warn {
    color: var(--rt-warn);
    font-size: var(--rt-size-small);
  }
  .result {
    color: var(--rt-ok);
    font-size: var(--rt-size-small);
  }
  .progress {
    color: var(--rt-fg-muted);
  }
</style>

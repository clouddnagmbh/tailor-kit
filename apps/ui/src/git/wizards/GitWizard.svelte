<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 14, strand G: one Git wizard in the dialog slot. The same flow for every action:
     1. the form (only what this action needs, names checked while typing);
     2. the plan, computed while typing (`GitApi.plan`, reads only): what happens in plain words,
        which refs move, which files change, the merge preview, backup and undo, warnings; when
        blocked, why and the way out (save, park changes, publish, choose);
     3. confirm (a destructive plan asks once more), `GitApi.apply`;
     4. the result with "Undo" (and "Resolve conflicts" when a merge stopped).
     Credentials (`git-auth`) are asked here and the action retried. After an action that
     changed files the app reloads its collections. "undo", "remotes", "clone", "diverged" and
     "detached" are wizards of their own around the same flow. -->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";

  import { BackendError } from "../../commands/backend";
  import type { GitAction, GitActionResult, GitCommit, GitPlan, GitRef, GitRemote, GitStash, GitStatus, GitUndoEntry } from "../../commands/types";
  import type { ExtensionDialogProps } from "../../extensions";
  import CloneFields from "../CloneFields.svelte";
  import { GLOSSARY, type GitTerm } from "../glossary";
  import { getGitViews, type GitViews } from "../views.svelte";
  import {
    confirmLabel,
    destructiveQuestion,
    initialValues,
    nameProblem,
    refMove,
    toAction,
    wayOut,
    wizardTerm,
    type WizardContext,
    type WizardValues,
  } from "./model";
  import type { GitWizardKind, GitWizardRequest } from "./open";
  import { suggestRefName } from "./refname";

  let { ctx, close, props }: ExtensionDialogProps = $props();
  const request = untrack(() => (props as unknown as { request: GitWizardRequest }).request);
  const app = untrack(() => ctx.app);
  const git = app.backend.git;
  let views: GitViews | null = null;
  try {
    views = getGitViews();
  } catch {
    views = null;
  }

  /** The wizard can turn into another (e.g. "Publish" from a blocked push). */
  let kind = $state<GitWizardKind>(request.kind);
  let loaded = $state(false);
  let status = $state<GitStatus | null>(null);
  let refs = $state<GitRef[]>([]);
  let commits = $state<GitCommit[]>([]);
  let remotes = $state<GitRemote[]>([]);
  let stashes = $state<GitStash[]>([]);
  let last = $state<GitUndoEntry | null>(null);
  let values = $state<WizardValues>(initialValues(request.kind, request.preset, { current: null, upstream: null, refs: [] }));
  let plan = $state<GitPlan | null>(null);
  let planError = $state<string | null>(null);
  let planning = $state(false);
  let step = $state<"form" | "confirm" | "running" | "done">("form");
  let result = $state<GitActionResult | null>(null);
  let error = $state<string | null>(null);
  let credentials = $state<{ host: string } | null>(null);
  let credUser = $state("");
  let credPassword = $state("");
  let cloneUrl = $state("");
  let explaining = $state(false);
  /** Remotes wizard: the sub-action being edited. */
  let remoteEdit = $state<GitWizardKind | null>(null);

  const wctx = $derived<WizardContext>({ current: status?.branch ?? null, upstream: status?.upstream ?? null, refs });
  const effective = $derived<GitWizardKind>(remoteEdit ?? kind);
  const action = $derived<GitAction | null>(toAction(effective, values, wctx));
  const problem = $derived(nameProblem(effective, values));
  const locals = $derived(refs.filter((r) => r.kind === "local"));
  const remoteRefs = $derived(refs.filter((r) => r.kind === "remote" && !r.name.endsWith("/HEAD")));
  const tags = $derived(refs.filter((r) => r.kind === "tag"));
  const term = $derived<GitTerm | null>(wizardTerm(effective));
  const describe = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function refsOf(): Promise<GitRef[]> {
    try {
      return await git.refs();
    } catch {
      const out: GitRef[] = [];
      for (const b of await git.branches()) {
        const target = await git.revParse(b.name).catch(() => null);
        if (target) {
          out.push({
            full: b.remote ? `refs/remotes/${b.name}` : `refs/heads/${b.name}`,
            name: b.name,
            kind: b.remote ? "remote" : "local",
            target,
            current: b.current,
            ahead: 0,
            behind: 0,
          });
        }
      }
      return out;
    }
  }

  async function load() {
    try {
      const s = await git.status();
      status = s;
      if (s.repo) {
        const [r, c, rem, st, la] = await Promise.all([
          refsOf(),
          git.log(30).catch(() => [] as GitCommit[]),
          git.remotes().catch(() => [] as GitRemote[]),
          git.stashes().catch(() => [] as GitStash[]),
          git.lastAction().catch(() => null),
        ]);
        refs = r;
        commits = c;
        remotes = rem;
        stashes = st;
        last = la;
      }
      values = initialValues(kind, request.preset, wctx);
      if (kind === "publish" && !request.preset?.remote) values.remote = remotes[0]?.name ?? "origin";
    } catch (e) {
      error = describe(e);
    }
    loaded = true;
  }

  onMount(() => {
    void load();
  });

  // Plans again whenever the action changes (debounced while typing).
  let planSeq = 0;
  $effect(() => {
    const a = action;
    const bad = problem;
    const ready = loaded && step === "form" && kind !== "undo" && kind !== "clone" && (kind !== "remotes" || remoteEdit !== null);
    const mine = ++planSeq;
    if (!ready || !a || bad) {
      untrack(() => {
        if (step === "form") {
          plan = null;
          planning = false;
        }
      });
      return;
    }
    untrack(() => (planning = true));
    const timer = setTimeout(() => {
      git.plan(a).then(
        (p) => {
          if (mine !== planSeq) return;
          plan = p;
          planError = null;
          planning = false;
        },
        (e: unknown) => {
          if (mine !== planSeq) return;
          plan = null;
          planError = describe(e);
          planning = false;
        },
      );
    }, 120);
    return () => clearTimeout(timer);
  });

  async function confirm() {
    if (!plan || plan.blocked || !action || planning) return;
    if (plan.destructive && step === "form") {
      step = "confirm";
      await tick();
      document.querySelector<HTMLButtonElement>('[data-testid="git-wizard-confirm-again"]')?.focus();
      return;
    }
    const a = action;
    await run(() => git.apply(a));
  }

  async function run(go: () => Promise<GitActionResult>) {
    step = "running";
    error = null;
    try {
      const r = await go();
      result = r;
      if (r.reload) await app.reload();
      step = "done";
      await tick();
      document.querySelector<HTMLButtonElement>('[data-testid="git-wizard-done"]')?.focus();
    } catch (e) {
      step = "form";
      if (e instanceof BackendError && e.kind === "git-auth") {
        credentials = { host: e.message };
        await tick();
        document.querySelector<HTMLInputElement>('[data-testid="git-wizard-cred-username"]')?.focus();
      } else {
        error = describe(e);
      }
    }
  }

  async function saveCredentials() {
    const c = credentials;
    if (!c) return;
    try {
      await git.setCredentials(c.host, credUser, credPassword);
    } catch (e) {
      error = describe(e);
      return;
    }
    credentials = null;
    credPassword = "";
    if (kind === "clone") await clone();
    else if (action) {
      const a = action;
      await run(() => git.apply(a));
    }
  }

  async function undoNow(entry: GitUndoEntry) {
    await run(() => git.undo(entry.id));
  }

  async function clone() {
    step = "running";
    error = null;
    try {
      const info = await git.clone(cloneUrl.trim());
      if (!info) {
        step = "form";
        return;
      }
      await app.adopt(info);
      result = { message: `Cloned into ${info.location}.`, reload: false };
      step = "done";
    } catch (e) {
      step = "form";
      if (e instanceof BackendError && e.kind === "git-auth") credentials = { host: e.message };
      else error = describe(e);
    }
  }

  /** The way out of a block. */
  async function escape() {
    const reason = plan?.blocked?.reason;
    if (!reason) return;
    error = null;
    switch (wayOut(reason)) {
      case "save":
        await app.save();
        values = { ...values };
        return;
      case "park":
        try {
          await git.apply({ action: "stash", message: `Parked before: ${confirmLabel(effective, plan, values).toLowerCase()}` });
          await app.reload();
          values = { ...values };
        } catch (e) {
          error = describe(e);
        }
        return;
      case "publish":
        kind = "publish";
        values = { ...initialValues("publish", {}, wctx), remote: remotes[0]?.name ?? "origin" };
        return;
      case "diverged":
        kind = "diverged";
        values = initialValues("diverged", {}, wctx);
        return;
      case "force":
        values = { ...values, force: true };
        return;
      case "resolve":
        close(undefined);
        views?.open({ kind: "merge" });
        return;
    }
  }

  const WAY_OUT_LABEL = {
    save: "Save all changes",
    park: "Park my changes first",
    publish: "Publish the branch",
    diverged: "Choose how to bring them together",
    force: "Delete anyway",
    resolve: "Resolve conflicts",
    none: "",
  } as const;

  function editRemote(sub: GitWizardKind, remote: GitRemote | null) {
    remoteEdit = sub;
    values = { ...values, remote: remote?.name ?? "", url: sub === "set-remote-url" ? (remote?.url ?? "") : "", name: sub === "add-remote" && remotes.length === 0 ? "origin" : "" };
  }

  const shortOid = (oid: string) => oid.slice(0, 7);
  const commitText = (c: GitCommit) => `${shortOid(c.oid)} · ${c.summary}`;
</script>

{#snippet refOptions(includeRemote: boolean, includeTags: boolean, exclude: string | null)}
  <optgroup label="Branches">
    {#each locals.filter((r) => r.name !== exclude) as r (r.full)}<option value={r.name}>{r.name}{r.current ? " (current)" : ""}</option>{/each}
  </optgroup>
  {#if includeRemote && remoteRefs.length > 0}
    <optgroup label="On the server">
      {#each remoteRefs as r (r.full)}<option value={r.name}>{r.name}</option>{/each}
    </optgroup>
  {/if}
  {#if includeTags && tags.length > 0}
    <optgroup label="Tags">
      {#each tags as r (r.full)}<option value={r.name}>{r.name}</option>{/each}
    </optgroup>
  {/if}
{/snippet}

{#snippet commitOptions()}
  {#each commits as c (c.oid)}<option value={c.oid}>{commitText(c)}</option>{/each}
{/snippet}

<div class="wizard" data-testid="git-wizard" data-kind={kind} data-step={step}>
  {#if term}
    <p class="lead">
      {GLOSSARY[term].text}
      <button class="link" data-testid="git-wizard-explain" aria-expanded={explaining} onclick={() => (explaining = !explaining)}>What does this mean?</button>
    </p>
    {#if explaining}
      <p class="gloss" data-testid="git-gloss">In Git this is called <code>{GLOSSARY[term].git}</code>. {GLOSSARY.backup.text}</p>
    {/if}
  {/if}

  {#if !loaded}
    <p class="hint">Reading the repository…</p>
  {:else if step === "done" && result}
    <section class="result" data-testid="git-wizard-result" aria-live="polite">
      <p class="result-message">{result.message}</p>
      {#if result.outcome?.outcome === "conflicts"}
        <p>Some fields were changed on both sides. Choose which version to keep, then commit.</p>
      {/if}
      <div class="buttons">
        {#if result.outcome?.outcome === "conflicts"}
          <button
            class="primary"
            data-testid="git-wizard-resolve"
            onclick={() => {
              close(result ?? undefined);
              views?.open({ kind: "merge" });
            }}>Resolve conflicts</button
          >
        {/if}
        {#if result.undo}
          <button data-testid="git-wizard-undo" onclick={() => result?.undo && undoNow(result.undo)}>Undo</button>
        {/if}
        <button class:primary={result.outcome?.outcome !== "conflicts"} data-testid="git-wizard-done" onclick={() => close(result ?? undefined)}>Done</button>
      </div>
    </section>
  {:else}
    {#if kind === "undo"}
      {#if !last}
        <p class="empty" data-testid="git-wizard-nothing">Nothing to undo: the app has not changed anything in Git yet.</p>
      {:else}
        <section class="plan" data-testid="git-wizard-plan">
          <h3>Undo “{last.summary}”</h3>
          <ul class="refs">
            {#each last.refs as m (m.name)}
              {@const r = refMove({ name: m.name, before: m.after, after: m.before })}
              <li><span class="ref-name">{r.name}</span> <code>{r.before}</code> → <code>{r.after}</code></li>
            {/each}
          </ul>
          {#if last.stale}<p class="blocked" data-testid="git-wizard-blocked">{last.stale}</p>{/if}
        </section>
      {/if}
    {:else if kind === "clone"}
      <form
        onsubmit={(e) => {
          e.preventDefault();
          if (cloneUrl.trim()) void clone();
        }}
      >
        <CloneFields bind:url={cloneUrl} disabled={step === "running"} />
      </form>
    {:else if kind === "remotes" && !remoteEdit}
      <section data-testid="git-wizard-remotes">
        {#if remotes.length === 0}
          <p class="empty">This repository is not linked to a server yet.</p>
        {:else}
          <ul class="remotes">
            {#each remotes as r (r.name)}
              <li data-testid="git-wizard-remote">
                <span class="remote-name">{r.name}</span>
                <span class="remote-url" title={r.url}>{r.url}</span>
                <span class="remote-actions">
                  <button onclick={() => editRemote("set-remote-url", r)}>Change address</button>
                  <button onclick={() => editRemote("rename-remote", r)}>Rename</button>
                  <button class="danger" onclick={() => editRemote("remove-remote", r)}>Remove</button>
                </span>
              </li>
            {/each}
          </ul>
        {/if}
        <button data-testid="git-wizard-add-remote" onclick={() => editRemote("add-remote", null)}>Add a remote…</button>
      </section>
    {:else}
      <form
        class="fields"
        novalidate
        onsubmit={(e) => {
          e.preventDefault();
          void confirm();
        }}
      >
        {#if effective === "create-branch"}
          <label
            >Name
            <!-- svelte-ignore a11y_autofocus -->
            <input
              data-testid="git-wizard-name"
              placeholder="feature/login"
              autofocus
              bind:value={values.name}
              oninput={() => {
                if (values.name.includes(" ")) values.name = suggestRefName(values.name);
              }}
            />
          </label>
          <label
            >Starts from
            <select data-testid="git-wizard-target" bind:value={values.target}>
              <option value="HEAD">Where you are now{status?.branch ? ` (${status.branch})` : ""}</option>
              {@render refOptions(true, true, null)}
              {#if values.target !== "HEAD" && !refs.some((r) => r.name === values.target)}<option value={values.target}>Commit {shortOid(values.target)}</option>{/if}
            </select>
          </label>
          <label class="check"><input type="checkbox" data-testid="git-wizard-switch" bind:checked={values.switchTo} /> Switch to it</label>
        {:else if effective === "switch"}
          <label>Switch to <select data-testid="git-wizard-target" bind:value={values.target}>{@render refOptions(true, true, status?.branch ?? null)}</select></label>
        {:else if effective === "rename-branch"}
          <label>Branch <select data-testid="git-wizard-from" bind:value={values.from}>{@render refOptions(false, false, null)}</select></label>
          <label
            >New name
            <!-- svelte-ignore a11y_autofocus -->
            <input data-testid="git-wizard-name" autofocus bind:value={values.name} />
          </label>
        {:else if effective === "delete-branch"}
          <label>Branch <select data-testid="git-wizard-branch" bind:value={values.name}>{@render refOptions(false, false, status?.branch ?? null)}</select></label>
        {:else if effective === "merge"}
          <label>Bring the changes of <select data-testid="git-wizard-from" bind:value={values.from}>{@render refOptions(true, true, status?.branch ?? null)}</select></label>
          <p class="hint">into <strong>{status?.branch ?? "the current branch"}</strong>.</p>
        {:else if effective === "publish"}
          <label>Branch <input data-testid="git-wizard-name" bind:value={values.name} /></label>
          <label
            >Server
            <select data-testid="git-wizard-remote" bind:value={values.remote}>
              {#each remotes as r (r.name)}<option value={r.name}>{r.name} · {r.url}</option>{/each}
            </select>
          </label>
        {:else if effective === "add-remote"}
          <label>Name <input data-testid="git-wizard-name" placeholder="origin" bind:value={values.name} /></label>
          <label
            >Address
            <!-- svelte-ignore a11y_autofocus -->
            <input data-testid="git-wizard-url" type="text" inputmode="url" placeholder="https://github.com/team/api.git" autofocus bind:value={values.url} />
          </label>
        {:else if effective === "rename-remote"}
          <label
            >New name for {values.remote}
            <!-- svelte-ignore a11y_autofocus -->
            <input data-testid="git-wizard-name" autofocus bind:value={values.name} />
          </label>
        {:else if effective === "set-remote-url"}
          <label
            >New address for {values.remote}
            <!-- svelte-ignore a11y_autofocus -->
            <input data-testid="git-wizard-url" type="text" inputmode="url" autofocus bind:value={values.url} />
          </label>
        {:else if effective === "stash"}
          <label
            >Note (optional)
            <!-- svelte-ignore a11y_autofocus -->
            <input data-testid="git-wizard-message" placeholder="What you were trying" autofocus bind:value={values.message} />
          </label>
        {:else if effective === "unstash" || effective === "drop-stash"}
          {#if stashes.length === 0}
            <p class="empty">There are no parked changes.</p>
          {:else}
            <fieldset class="choices">
              <legend>Parked changes</legend>
              {#each stashes as s (s.oid)}
                <label class="choice">
                  <input type="radio" name="stash" value={s.index} data-testid="git-wizard-stash" bind:group={values.index} />
                  <span><strong>{s.message}</strong> <span class="hint">{s.files.length} {s.files.length === 1 ? "file" : "files"}{s.branch ? ` · on ${s.branch}` : ""}</span></span>
                </label>
              {/each}
            </fieldset>
            {#if effective === "unstash"}<label class="check"><input type="checkbox" bind:checked={values.keep} /> Keep them in the list too</label>{/if}
          {/if}
        {:else if effective === "revert" || effective === "cherry-pick"}
          <label
            >Commit
            <select data-testid="git-wizard-commit" bind:value={values.commit}>
              {#if !values.commit}<option value="">Choose a commit…</option>{/if}
              {#if values.commit && !commits.some((c) => c.oid === values.commit)}<option value={values.commit}>{shortOid(values.commit)}</option>{/if}
              {@render commitOptions()}
            </select>
          </label>
        {:else if effective === "reset"}
          <label
            >Move {status?.branch ?? "the branch"} to
            <select data-testid="git-wizard-target" bind:value={values.target}>
              <option value="HEAD~1">One commit back</option>
              {#if status?.upstream}<option value={status.upstream}>Where {status.upstream} is</option>{/if}
              {@render commitOptions()}
            </select>
          </label>
          <fieldset class="choices">
            <legend>Your files</legend>
            <label class="choice"
              ><input type="radio" name="mode" value="keep-changes" bind:group={values.mode} />
              <span><strong>Keep my work</strong> <span class="hint">what the commits did shows as changes</span></span></label
            >
            <label class="choice"
              ><input type="radio" name="mode" value="discard" data-testid="git-wizard-mode-discard" bind:group={values.mode} />
              <span><strong>Throw it away</strong> <span class="hint">files match that commit (a backup is kept)</span></span></label
            >
          </fieldset>
        {:else if effective === "discard"}
          <p>{values.paths.length === 0 ? "All changed files" : `${values.paths.length} ${values.paths.length === 1 ? "file" : "files"}`} go back to the last commit.</p>
        {:else if effective === "create-tag"}
          <label
            >Name
            <!-- svelte-ignore a11y_autofocus -->
            <input data-testid="git-wizard-name" placeholder="v1.2.0" autofocus bind:value={values.name} />
          </label>
          <label
            >Commit
            <select data-testid="git-wizard-target" bind:value={values.target}>
              <option value="HEAD">Where you are now</option>
              {#if values.target !== "HEAD" && !commits.some((c) => c.oid === values.target)}<option value={values.target}>{shortOid(values.target)}</option>{/if}
              {@render commitOptions()}
            </select>
          </label>
          <label>Message (optional) <input data-testid="git-wizard-message" placeholder="Release notes in one line" bind:value={values.message} /></label>
        {:else if effective === "delete-tag" || effective === "push-tag"}
          <label
            >Tag
            <select data-testid="git-wizard-tag" bind:value={values.name}>
              {#if !values.name}<option value="">Choose a tag…</option>{/if}
              {#each tags as t (t.full)}<option value={t.name}>{t.name}</option>{/each}
            </select>
          </label>
        {:else if effective === "diverged"}
          <fieldset class="choices">
            <legend>{status?.branch} and {status?.upstream} went apart: you have {status?.ahead} new, the server has {status?.behind}.</legend>
            <label class="choice"
              ><input type="radio" name="choice" value="merge" data-testid="git-wizard-choice-merge" bind:group={values.choice} />
              <span><strong>Merge both</strong> <span class="hint">keeps everyone's commits (recommended)</span></span></label
            >
            <label class="choice"
              ><input type="radio" name="choice" value="theirs" data-testid="git-wizard-choice-theirs" bind:group={values.choice} />
              <span><strong>Take theirs</strong> <span class="hint">your commits leave the branch; a backup keeps them</span></span></label
            >
          </fieldset>
          <p class="hint">“Keep mine” would overwrite the server; that is only possible on the command line.</p>
        {:else if effective === "detached"}
          <fieldset class="choices">
            <legend>You are looking at an older commit, not at a branch.</legend>
            <label class="choice"
              ><input type="radio" name="choice" value="branch" data-testid="git-wizard-choice-branch" bind:group={values.choice} />
              <span><strong>Create a branch here</strong> <span class="hint">to keep working from this commit</span></span></label
            >
            <label class="choice"
              ><input type="radio" name="choice" value="back" data-testid="git-wizard-choice-back" bind:group={values.choice} /> <span><strong>Go back to a branch</strong></span></label
            >
          </fieldset>
          {#if values.choice === "branch"}
            <label>Name <input data-testid="git-wizard-name" placeholder="feature/name" bind:value={values.name} /></label>
          {:else}
            <label>Branch <select data-testid="git-wizard-target" bind:value={values.target}>{@render refOptions(false, false, null)}</select></label>
          {/if}
        {:else if effective === "remove-remote"}
          <p>Remove the link to <strong>{values.remote}</strong>?</p>
        {/if}
        {#if problem}<p class="field-error" data-testid="git-wizard-name-problem">{problem}</p>{/if}
      </form>

      {#if plan}
        <section class="plan" data-testid="git-wizard-plan" aria-live="polite">
          <h3>What happens</h3>
          <ol class="steps">
            {#each plan.steps as s, i (i)}<li>{s}</li>{/each}
          </ol>
          {#if plan.merge}
            <div class="merge" data-testid="git-wizard-merge">
              <p class="hint">
                {plan.merge.commits.length}
                {plan.merge.commits.length === 1 ? "commit" : "commits"} · {plan.merge.files.length}
                {plan.merge.files.length === 1 ? "file" : "files"}{plan.merge.fast_forward ? " · moves forward" : ""}
              </p>
              <ul class="files">
                {#each plan.merge.files as f (f.path)}
                  <li data-testid="git-wizard-merge-file" class:conflict={f.conflicts.length > 0}>
                    <span class="path">{f.path}</span>
                    {#if f.conflicts.length > 0}
                      <span class="tag bad" title={f.conflicts.join(", ")}>{f.conflicts.length} to decide</span>
                    {:else}
                      <span class="tag ok">{f.semantic ? "merges by field" : "merges cleanly"}</span>
                    {/if}
                  </li>
                {/each}
              </ul>
            </div>
          {:else if plan.files && plan.files.length > 0}
            <details class="files-box">
              <summary>{plan.files.length} {plan.files.length === 1 ? "file changes" : "files change"}</summary>
              <ul class="files">
                {#each plan.files as f (f)}<li class="path">{f}</li>{/each}
              </ul>
            </details>
          {/if}
          {#if plan.refs.length > 0}
            <ul class="refs" aria-label="What moves">
              {#each plan.refs as m (m.name)}
                {@const r = refMove(m)}
                <li><span class="ref-name">{r.name}</span> <code>{r.before}</code> → <code>{r.after}</code></li>
              {/each}
            </ul>
          {/if}
          <p class="facts">
            {#if plan.backup}<span data-testid="git-wizard-backup">Backup kept</span>{/if}
            {#if plan.undoable}<span>Can be undone</span>{:else}<span data-testid="git-wizard-not-undoable">Cannot be undone</span>{/if}
            {#if plan.network}<span>Talks to the server</span>{/if}
          </p>
          {#each plan.warnings ?? [] as w (w)}<p class="warning" data-testid="git-wizard-warning">{w}</p>{/each}
          {#if plan.blocked}
            <div class="blocked" data-testid="git-wizard-blocked" data-reason={plan.blocked.reason} role="alert">
              <p>{plan.blocked.message}</p>
              {#if wayOut(plan.blocked.reason) !== "none"}
                <button data-testid="git-wizard-way-out" onclick={escape}>{WAY_OUT_LABEL[wayOut(plan.blocked.reason)]}</button>
              {/if}
            </div>
          {/if}
        </section>
      {:else if planError}
        <p class="error" role="alert">{planError}</p>
      {:else if planning}
        <p class="hint">Checking…</p>
      {/if}
    {/if}

    {#if credentials}
      <form
        class="fields cred"
        data-testid="git-wizard-credentials"
        onsubmit={(e) => {
          e.preventDefault();
          void saveCredentials();
        }}
      >
        <p><strong>{credentials.host}</strong> needs a username and password (or access token).</p>
        <label>Username <input data-testid="git-wizard-cred-username" autocomplete="username" bind:value={credUser} /></label>
        <label>Password or token <input data-testid="git-wizard-cred-password" type="password" autocomplete="current-password" bind:value={credPassword} /></label>
        <button class="primary" type="submit" data-testid="git-wizard-cred-save" disabled={!credUser || !credPassword}>Save and retry</button>
      </form>
    {/if}

    {#if error}<p class="error" role="alert" data-testid="git-wizard-error">{error}</p>{/if}

    {#if step === "confirm" && plan}
      <div class="again" role="group" aria-label="Confirm" data-testid="git-wizard-again">
        <p>{destructiveQuestion(effective, plan)}</p>
        <div class="buttons">
          <button onclick={() => (step = "form")}>Back</button>
          <button
            class="primary danger"
            data-testid="git-wizard-confirm-again"
            onclick={() => {
              const a = action;
              if (a) void run(() => git.apply(a));
            }}>Yes, {confirmLabel(effective, plan, values).toLowerCase()}</button
          >
        </div>
      </div>
    {:else}
      <div class="buttons">
        {#if remoteEdit}
          <button
            onclick={() => {
              remoteEdit = null;
              plan = null;
            }}>Back</button
          >
        {/if}
        <button data-testid="git-wizard-cancel" onclick={() => close(undefined)}>{kind === "remotes" && !remoteEdit ? "Close" : "Cancel"}</button>
        {#if kind === "undo"}
          <button class="primary" data-testid="git-wizard-confirm" disabled={!last || !!last.stale || step === "running"} onclick={() => last && undoNow(last)}>Undo</button>
        {:else if kind === "clone"}
          <button class="primary" data-testid="git-wizard-confirm" disabled={!cloneUrl.trim() || step === "running"} onclick={clone}>{step === "running" ? "Cloning…" : "Clone"}</button>
        {:else if kind !== "remotes" || remoteEdit}
          <button
            class="primary"
            class:danger={plan?.destructive}
            data-testid="git-wizard-confirm"
            disabled={!plan || !!plan.blocked || planning || step === "running" || !!problem}
            onclick={confirm}>{step === "running" ? "Working…" : confirmLabel(effective, plan, values)}</button
          >
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .wizard {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
    margin-top: 8px;
    font-size: var(--rt-size);
  }
  p {
    margin: 0;
  }
  .lead,
  .hint {
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .gloss {
    font-size: var(--rt-size-small);
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  label.check,
  label.choice {
    flex-direction: row;
    align-items: baseline;
    gap: 8px;
    color: var(--rt-fg);
  }
  fieldset {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    border: none;
  }
  legend {
    margin-bottom: 6px;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
  }
  input:not([type="checkbox"], [type="radio"]),
  select {
    min-width: 0;
    box-sizing: border-box;
    padding: 5px 8px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font: inherit;
  }
  .field-error,
  .error {
    color: var(--rt-danger);
    font-size: var(--rt-size-small);
  }
  .plan {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg-panel);
  }
  h3 {
    margin: 0;
    font-size: var(--rt-size-small);
    color: var(--rt-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .steps {
    margin: 0;
    padding-left: 18px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .files li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }
  .path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
  }
  .tag {
    flex: none;
    font-size: var(--rt-size-small);
  }
  .tag.ok {
    color: var(--rt-ok);
  }
  .tag.bad {
    color: var(--rt-danger);
  }
  .refs {
    font-size: var(--rt-size-small);
  }
  .ref-name {
    font-weight: 600;
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .warning {
    color: var(--rt-warn);
    font-size: var(--rt-size-small);
  }
  .blocked {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    color: var(--rt-danger);
    font-size: var(--rt-size-small);
  }
  .remotes li {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--rt-border);
  }
  .remote-name {
    font-weight: 600;
  }
  .remote-url {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--rt-fg-muted);
  }
  .remote-actions {
    grid-column: 1 / -1;
    display: flex;
    gap: 6px;
  }
  .again {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--rt-danger);
    border-radius: var(--rt-radius);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
  }
  button {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg);
    color: var(--rt-fg);
    font: inherit;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  button.primary:not(:disabled) {
    background: var(--rt-accent);
    border-color: var(--rt-accent);
    color: var(--rt-accent-fg);
  }
  button.primary.danger:not(:disabled) {
    background: var(--rt-danger);
    border-color: var(--rt-danger);
  }
  button.danger:not(.primary) {
    color: var(--rt-danger);
  }
  .link {
    height: auto;
    padding: 0;
    border: none;
    background: none;
    color: var(--rt-accent);
    font-size: var(--rt-size-small);
  }
  .empty {
    color: var(--rt-fg-muted);
  }
  .result {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .result-message {
    font-weight: 600;
  }
</style>

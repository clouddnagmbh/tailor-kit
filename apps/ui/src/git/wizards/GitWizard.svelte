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
  import { Badge, Banner, Button, Card, Checkbox, Chip, DialogFooter, EmptyState, Icon, Select, Spinner, TextField, type SelectOption } from "../../ui";
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
    // The dialog put focus in before the form existed: move it to the first field now.
    await tick();
    root?.querySelector<HTMLElement>("[autofocus], input:not([type=checkbox]), select, [role=button]")?.focus();
  }

  let root = $state<HTMLElement>();

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

  const branchOpts = (exclude: string | null): SelectOption[] =>
    locals.filter((r) => r.name !== exclude).map((r) => ({ value: r.name, label: r.current ? `${r.name} (current)` : r.name }));
  const remoteOpts = $derived<SelectOption[]>(remoteRefs.map((r) => ({ value: r.name, label: `${r.name} (on the server)` })));
  const tagOpts = $derived<SelectOption[]>(tags.map((r) => ({ value: r.name, label: `${r.name} (tag)` })));
  const commitOpts = $derived<SelectOption[]>(commits.map((c) => ({ value: c.oid, label: commitText(c) })));
  /** Keeps a preset value (a commit id) selectable when it is not in the lists. */
  function withValue(opts: SelectOption[], value: string, label: (v: string) => string): SelectOption[] {
    return value && !opts.some((o) => o.value === value) ? [{ value, label: label(value) }, ...opts] : opts;
  }
  const busy = $derived(step === "running");
</script>

{#snippet choice(testid: string, selected: boolean, title: string, text: string, pick: () => void)}
  <div class="choice" data-testid={testid} data-selected={selected}>
    <Card padding="sm" {selected} {title} description={text} onclick={pick} />
  </div>
{/snippet}

{#snippet nameField(placeholder: string, hint?: string)}
  <!-- svelte-ignore a11y_autofocus -->
  <TextField
    label="Name"
    mono
    autofocus
    {placeholder}
    {hint}
    error={problem ?? undefined}
    data-testid="git-wizard-name"
    bind:value={values.name}
    oninput={() => {
      if (/\s/.test(values.name)) values.name = suggestRefName(values.name);
    }}
  />
{/snippet}

<div class="wizard" bind:this={root} data-testid="git-wizard" data-kind={kind} data-step={step}>
  {#if term && step !== "done"}
    <p class="lead">{GLOSSARY[term].text}</p>
    {#if explaining}
      <p class="gloss" data-testid="git-gloss">In Git this is called <code>{GLOSSARY[term].git}</code>. {GLOSSARY.backup.text}</p>
    {/if}
  {/if}

  {#if !loaded}
    <div class="waiting"><Spinner size="sm" /> <span class="muted">Reading the repository…</span></div>
  {:else if step === "done" && result}
    <section class="result" data-testid="git-wizard-result" aria-live="polite">
      <span class="result-icon" class:warn={result.outcome?.outcome === "conflicts"}>
        <Icon name={result.outcome?.outcome === "conflicts" ? "warning" : "success"} size="lg" />
      </span>
      <div class="result-text">
        <p class="result-message">{result.message}</p>
        {#if result.outcome?.outcome === "conflicts"}
          <p class="muted">Some fields were changed on both sides. Choose which version to keep, then commit.</p>
        {:else if result.undo}
          <p class="muted">Changed your mind? Undo puts everything back as it was.</p>
        {/if}
      </div>
    </section>
    <DialogFooter>
      {#if result.undo}
        <Button icon="undo" data-testid="git-wizard-undo" onclick={() => result?.undo && undoNow(result.undo)}>Undo</Button>
      {/if}
      {#if result.outcome?.outcome === "conflicts"}
        <Button data-testid="git-wizard-done" onclick={() => close(result ?? undefined)}>Later</Button>
        <Button
          variant="primary"
          icon="merge"
          data-testid="git-wizard-resolve"
          onclick={() => {
            close(result ?? undefined);
            views?.open({ kind: "merge" });
          }}>Resolve conflicts</Button
        >
      {:else}
        <Button variant="primary" data-testid="git-wizard-done" onclick={() => close(result ?? undefined)}>Done</Button>
      {/if}
    </DialogFooter>
  {:else}
    {#if kind === "undo"}
      {#if !last}
        <div data-testid="git-wizard-nothing">
          <EmptyState icon="undo" title="Nothing to undo" text="The app has not changed anything in Git yet." size="sm" />
        </div>
      {:else}
        <section class="plan" data-testid="git-wizard-plan">
          <h3 class="caption">Undo “{last.summary}”</h3>
          <ul class="refs">
            {#each last.refs as m (m.name)}
              {@const r = refMove({ name: m.name, before: m.after, after: m.before })}
              <li><Chip size="sm" mono icon="branch" label={r.name} /> <code>{r.before}</code> <Icon name="chevron-right" size="sm" /> <code>{r.after}</code></li>
            {/each}
          </ul>
        </section>
        {#if last.stale}
          <div data-testid="git-wizard-blocked"><Banner tone="warn" title="Cannot undo any more">{last.stale}</Banner></div>
        {/if}
      {/if}
    {:else if kind === "clone"}
      <form
        onsubmit={(e) => {
          e.preventDefault();
          if (cloneUrl.trim()) void clone();
        }}
      >
        <CloneFields bind:url={cloneUrl} disabled={busy} />
      </form>
    {:else if kind === "remotes" && !remoteEdit}
      <section class="stack" data-testid="git-wizard-remotes">
        {#if remotes.length === 0}
          <EmptyState icon="service" title="No server yet" text="Link this repository to GitHub, GitLab or any Git server to share it." size="sm" />
        {:else}
          {#each remotes as r (r.name)}
            <div data-testid="git-wizard-remote">
              <Card padding="sm" icon="service" title={r.name} description={r.url}>
                {#snippet footer()}
                  <Button size="sm" variant="ghost" icon="edit" onclick={() => editRemote("set-remote-url", r)}>Change address</Button>
                  <Button size="sm" variant="ghost" onclick={() => editRemote("rename-remote", r)}>Rename</Button>
                  <Button size="sm" variant="ghost" icon="delete" onclick={() => editRemote("remove-remote", r)}>Remove</Button>
                {/snippet}
              </Card>
            </div>
          {/each}
        {/if}
      </section>
    {:else}
      <form
        class="stack"
        novalidate
        onsubmit={(e) => {
          e.preventDefault();
          void confirm();
        }}
      >
        {#if effective === "create-branch"}
          {@render nameField("feature/login", "Spaces become dashes.")}
          <Select
            label="Starts from"
            data-testid="git-wizard-target"
            bind:value={values.target}
            options={withValue(
              [{ value: "HEAD", label: `Where you are now${status?.branch ? ` (${status.branch})` : ""}` }, ...branchOpts(null), ...remoteOpts, ...tagOpts],
              values.target,
              (v) => `Commit ${shortOid(v)}`,
            )}
          />
          <Checkbox data-testid="git-wizard-switch" label="Switch to it" description="Your unsaved and uncommitted work comes along." bind:checked={values.switchTo} />
        {:else if effective === "switch"}
          <Select label="Switch to" data-testid="git-wizard-target" bind:value={values.target} options={[...branchOpts(status?.branch ?? null), ...remoteOpts, ...tagOpts]} />
        {:else if effective === "rename-branch"}
          <Select label="Branch" data-testid="git-wizard-from" bind:value={values.from} options={branchOpts(null)} />
          <!-- svelte-ignore a11y_autofocus -->
          <TextField label="New name" mono autofocus error={problem ?? undefined} data-testid="git-wizard-name" bind:value={values.name} />
        {:else if effective === "delete-branch"}
          <Select label="Branch" data-testid="git-wizard-branch" bind:value={values.name} options={branchOpts(status?.branch ?? null)} />
        {:else if effective === "merge"}
          <Select
            label="Bring the changes of"
            hint="into {status?.branch ?? 'the current branch'}"
            data-testid="git-wizard-from"
            bind:value={values.from}
            options={[...branchOpts(status?.branch ?? null), ...remoteOpts, ...tagOpts]}
          />
        {:else if effective === "publish"}
          <TextField label="Branch" mono data-testid="git-wizard-name" bind:value={values.name} />
          <Select label="Server" data-testid="git-wizard-remote" bind:value={values.remote} options={remotes.map((r) => ({ value: r.name, label: `${r.name} · ${r.url}` }))} />
        {:else if effective === "add-remote"}
          <TextField label="Name" mono placeholder="origin" hint="“origin” is the usual name for the main server." error={problem ?? undefined} data-testid="git-wizard-name" bind:value={values.name} />
          <!-- svelte-ignore a11y_autofocus -->
          <TextField label="Address" mono autofocus inputmode="url" placeholder="https://github.com/team/api.git" data-testid="git-wizard-url" bind:value={values.url} />
        {:else if effective === "rename-remote"}
          <!-- svelte-ignore a11y_autofocus -->
          <TextField label="New name for {values.remote}" mono autofocus error={problem ?? undefined} data-testid="git-wizard-name" bind:value={values.name} />
        {:else if effective === "set-remote-url"}
          <!-- svelte-ignore a11y_autofocus -->
          <TextField label="New address for {values.remote}" mono autofocus inputmode="url" data-testid="git-wizard-url" bind:value={values.url} />
        {:else if effective === "stash"}
          <!-- svelte-ignore a11y_autofocus -->
          <TextField label="Note (optional)" autofocus placeholder="What you were trying" data-testid="git-wizard-message" bind:value={values.message} />
        {:else if effective === "unstash" || effective === "drop-stash"}
          {#if stashes.length === 0}
            <EmptyState icon="stash" title="No parked changes" text="Changes you park appear here until you bring them back." size="sm" />
          {:else}
            <div class="choices" role="group" aria-label="Parked changes">
              {#each stashes as s (s.oid)}
                {@render choice(
                  "git-wizard-stash",
                  values.index === s.index,
                  s.message,
                  `${s.files.length} ${s.files.length === 1 ? "file" : "files"}${s.branch ? ` · on ${s.branch}` : ""}`,
                  () => (values.index = s.index),
                )}
              {/each}
            </div>
            {#if effective === "unstash"}<Checkbox label="Keep them in the list too" bind:checked={values.keep} />{/if}
          {/if}
        {:else if effective === "revert" || effective === "cherry-pick"}
          <Select
            label="Commit"
            data-testid="git-wizard-commit"
            bind:value={values.commit}
            options={withValue(values.commit ? commitOpts : [{ value: "", label: "Choose a commit…" }, ...commitOpts], values.commit, shortOid)}
          />
        {:else if effective === "reset"}
          <Select
            label="Move {status?.branch ?? 'the branch'} to"
            data-testid="git-wizard-target"
            bind:value={values.target}
            options={withValue(
              [{ value: "HEAD~1", label: "One commit back" }, ...(status?.upstream ? [{ value: status.upstream, label: `Where ${status.upstream} is` }] : []), ...commitOpts],
              values.target,
              shortOid,
            )}
          />
          <div class="choices" role="group" aria-label="Your files">
            {@render choice("git-wizard-mode-keep", values.mode === "keep-changes", "Keep my work", "What the commits did shows up as changes.", () => (values.mode = "keep-changes"))}
            {@render choice("git-wizard-mode-discard", values.mode === "discard", "Throw it away", "Files match that commit. A backup is kept.", () => (values.mode = "discard"))}
          </div>
        {:else if effective === "discard"}
          <p>
            {values.paths.length === 0 ? "All changed files" : `${values.paths.length} ${values.paths.length === 1 ? "file" : "files"}`} go back to how they were in the last commit.
          </p>
        {:else if effective === "create-tag"}
          {@render nameField("v1.2.0")}
          <Select
            label="Commit"
            data-testid="git-wizard-target"
            bind:value={values.target}
            options={withValue([{ value: "HEAD", label: "Where you are now" }, ...commitOpts], values.target, shortOid)}
          />
          <TextField label="Message (optional)" placeholder="Release notes in one line" data-testid="git-wizard-message" bind:value={values.message} />
        {:else if effective === "delete-tag" || effective === "push-tag"}
          <Select label="Tag" data-testid="git-wizard-tag" bind:value={values.name} options={values.name ? tagOpts : [{ value: "", label: "Choose a tag…" }, ...tagOpts]} />
        {:else if effective === "diverged"}
          <p class="muted">{status?.branch} and {status?.upstream} went apart: you have {status?.ahead} new, the server has {status?.behind}.</p>
          <div class="choices" role="group" aria-label="How to bring them together">
            {@render choice("git-wizard-choice-merge", values.choice === "merge", "Merge both (recommended)", "Keeps everyone's commits.", () => (values.choice = "merge"))}
            {@render choice("git-wizard-choice-theirs", values.choice === "theirs", "Take theirs", "Your commits leave the branch; a backup keeps them.", () => (values.choice = "theirs"))}
          </div>
          <p class="muted small">“Keep mine” would overwrite the server; that is only possible on the command line.</p>
        {:else if effective === "detached"}
          <div class="choices" role="group" aria-label="Where to go">
            {@render choice("git-wizard-choice-branch", values.choice === "branch", "Create a branch here", "Keep working from this commit.", () => (values.choice = "branch"))}
            {@render choice("git-wizard-choice-back", values.choice === "back", "Go back to a branch", "Leave this older commit.", () => (values.choice = "back"))}
          </div>
          {#if values.choice === "branch"}
            {@render nameField("feature/name")}
          {:else}
            <Select label="Branch" data-testid="git-wizard-target" bind:value={values.target} options={branchOpts(null)} />
          {/if}
        {:else if effective === "remove-remote"}
          <p>Remove the link to <strong>{values.remote}</strong>? The branches on the server stay where they are.</p>
        {/if}
        <button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
      </form>

      {#if plan && (plan.steps.length > 0 || !plan.blocked)}
        <section class="plan" data-testid="git-wizard-plan" aria-live="polite">
          <h3 class="caption">What happens</h3>
          <ol class="steps">
            {#each plan.steps as s, i (i)}<li>{s}</li>{/each}
          </ol>
          {#if plan.merge}
            <div class="merge" data-testid="git-wizard-merge">
              <p class="muted small">
                {plan.merge.commits.length}
                {plan.merge.commits.length === 1 ? "commit" : "commits"} · {plan.merge.files.length}
                {plan.merge.files.length === 1 ? "file" : "files"}{plan.merge.fast_forward ? " · moves forward" : ""}
              </p>
              <ul class="files">
                {#each plan.merge.files as f (f.path)}
                  <li data-testid="git-wizard-merge-file" class:conflict={f.conflicts.length > 0}>
                    <span class="path">{f.path}</span>
                    {#if f.conflicts.length > 0}
                      <Badge kind="label" tone="danger" value="{f.conflicts.length} to decide" title={f.conflicts.join(", ")} />
                    {:else}
                      <Badge kind="label" tone="ok" value={f.semantic ? "merges by field" : "merges cleanly"} />
                    {/if}
                  </li>
                {/each}
              </ul>
            </div>
          {:else if plan.files && plan.files.length > 0}
            <details class="files-box">
              <summary>{plan.files.length} {plan.files.length === 1 ? "file changes" : "files change"}</summary>
              <ul class="files">
                {#each plan.files as f (f)}<li><span class="path">{f}</span></li>{/each}
              </ul>
            </details>
          {/if}
          {#if plan.refs.length > 0}
            <ul class="refs" aria-label="What moves">
              {#each plan.refs as m (m.name)}
                {@const r = refMove(m)}
                <li><Chip size="sm" mono icon={m.name.startsWith("refs/tags/") ? "tag" : "branch"} label={r.name} /> <code>{r.before}</code> <Icon name="chevron-right" size="sm" /> <code>{r.after}</code></li>
              {/each}
            </ul>
          {/if}
          <div class="facts" hidden={!!plan.blocked}>
            {#if plan.backup}<span data-testid="git-wizard-backup"><Badge kind="label" tone="ok" value="Backup kept" /></span>{/if}
            {#if plan.undoable}<Badge kind="label" tone="info" value="Can be undone" />{:else}<span data-testid="git-wizard-not-undoable"><Badge kind="label" tone="warn" value="Cannot be undone" /></span>{/if}
            {#if plan.network}<Badge kind="label" value="Talks to the server" />{/if}
          </div>
        </section>
      {/if}
      {#if plan}
        {#each plan.warnings ?? [] as w (w)}
          <div data-testid="git-wizard-warning"><Banner tone="warn">{w}</Banner></div>
        {/each}
        {#if plan.blocked}
          {@const out = wayOut(plan.blocked.reason)}
          <div data-testid="git-wizard-blocked" data-reason={plan.blocked.reason}>
            <Banner tone={plan.blocked.reason === "nothing-to-do" ? "info" : "warn"} title={plan.blocked.reason === "nothing-to-do" ? "Nothing to do" : "Not possible yet"}>
              {plan.blocked.message}
              {#if out !== "none"}
                <span class="banner-actions"><Button size="sm" data-testid="git-wizard-way-out" onclick={escape}>{WAY_OUT_LABEL[out]}</Button></span>
              {/if}
            </Banner>
          </div>
        {/if}
      {:else if planError}
        <Banner tone="danger">{planError}</Banner>
      {:else if planning}
        <div class="waiting"><Spinner size="sm" /> <span class="muted">Checking…</span></div>
      {/if}
    {/if}

    {#if credentials}
      <form
        class="stack"
        data-testid="git-wizard-credentials"
        onsubmit={(e) => {
          e.preventDefault();
          void saveCredentials();
        }}
      >
        <Card padding="sm" icon="key" title="Sign in to {credentials.host}" description="A username and password, or an access token. Stored on this device only.">
          <div class="stack">
            <TextField label="Username" data-testid="git-wizard-cred-username" autocomplete="username" bind:value={credUser} />
            <TextField label="Password or token" type="password" data-testid="git-wizard-cred-password" autocomplete="current-password" bind:value={credPassword} />
          </div>
          {#snippet footer()}
            <Button size="sm" variant="primary" type="submit" data-testid="git-wizard-cred-save" disabled={!credUser || !credPassword}>Save and retry</Button>
          {/snippet}
        </Card>
      </form>
    {/if}

    {#if error}<div data-testid="git-wizard-error"><Banner tone="danger">{error}</Banner></div>{/if}

    {#if step === "confirm" && plan}
      <div class="again" data-testid="git-wizard-again">
        <Banner tone="danger" title="Are you sure?">{destructiveQuestion(effective, plan)}</Banner>
      </div>
      <DialogFooter>
        <Button onclick={() => (step = "form")}>Back</Button>
        <Button
          variant="danger"
          data-testid="git-wizard-confirm-again"
          onclick={() => {
            const a = action;
            if (a) void run(() => git.apply(a));
          }}>Yes, {confirmLabel(effective, plan, values).toLowerCase()}</Button
        >
      </DialogFooter>
    {:else}
      <DialogFooter>
        {#snippet start()}
          {#if term}
            <Button variant="ghost" size="sm" icon="help" data-testid="git-wizard-explain" aria-expanded={explaining} onclick={() => (explaining = !explaining)}>What does this mean?</Button>
          {/if}
        {/snippet}
        {#if remoteEdit}
          <Button
            variant="ghost"
            icon="chevron-left"
            onclick={() => {
              remoteEdit = null;
              plan = null;
            }}>Back</Button
          >
        {/if}
        {#if kind === "remotes" && !remoteEdit}
          <Button data-testid="git-wizard-cancel" onclick={() => close(undefined)}>Close</Button>
          <Button variant="primary" icon="add" data-testid="git-wizard-add-remote" onclick={() => editRemote("add-remote", null)}>Add a remote…</Button>
        {:else}
          <Button data-testid="git-wizard-cancel" onclick={() => close(undefined)}>Cancel</Button>
          {#if kind === "undo"}
            <Button variant="primary" icon="undo" data-testid="git-wizard-confirm" disabled={!last || !!last.stale || busy} loading={busy} onclick={() => last && undoNow(last)}>Undo</Button>
          {:else if kind === "clone"}
            <Button variant="primary" data-testid="git-wizard-confirm" disabled={!cloneUrl.trim() || busy} loading={busy} onclick={clone}>Clone</Button>
          {:else}
            <!-- The first click never destroys: a destructive plan asks once more (danger button). -->
            <Button
              variant="primary"
              data-testid="git-wizard-confirm"
              data-destructive={plan?.destructive ?? false}
              disabled={!plan || !!plan.blocked || planning || busy || !!problem}
              loading={busy}
              onclick={confirm}>{confirmLabel(effective, plan, values)}</Button
            >
          {/if}
        {/if}
      </DialogFooter>
    {/if}
  {/if}
</div>

<style>
  .wizard {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-6);
    min-width: 0;
    font: var(--rt-font-body);
  }
  p {
    margin: 0;
  }
  .lead,
  .muted {
    color: var(--rt-fg-muted);
  }
  .small {
    font: var(--rt-font-small);
  }
  .gloss {
    padding: var(--rt-space-4) var(--rt-space-5);
    border-radius: var(--rt-radius-md);
    background: var(--rt-surface-sunken);
    font: var(--rt-font-small);
  }
  .waiting {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
  }
  .choices {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--rt-space-4);
  }
  .choice {
    display: flex;
    min-width: 0;
  }
  .plan {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
    padding: var(--rt-space-5) var(--rt-space-6);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-sunken);
  }
  .caption {
    margin: 0;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    letter-spacing: var(--rt-tracking-caps);
    text-transform: uppercase;
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-2);
    margin: 0;
    padding-left: var(--rt-space-6);
  }
  .files,
  .refs {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .files li,
  .refs li {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    min-width: 0;
  }
  .files li {
    justify-content: space-between;
  }
  .path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
  }
  .refs code {
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
  }
  .files-box summary {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    cursor: pointer;
  }
  .files-box .files {
    margin-top: var(--rt-space-3);
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-3);
  }
  .banner-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-3);
    margin-top: var(--rt-space-4);
  }
  .result {
    display: flex;
    align-items: flex-start;
    gap: var(--rt-space-5);
  }
  .result-icon {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: var(--rt-control-lg);
    height: var(--rt-control-lg);
    border-radius: var(--rt-radius-pill);
    background: var(--rt-ok-bg);
    color: var(--rt-ok-fg);
  }
  .result-icon.warn {
    background: var(--rt-warn-bg);
    color: var(--rt-warn-fg);
  }
  .result-text {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-2);
    min-width: 0;
  }
  .result-message {
    font: var(--rt-font-title);
  }
</style>

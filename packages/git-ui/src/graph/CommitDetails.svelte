<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 14, strand F: the details of the selected commit (or of two marked commits): message,
     author, dates, id, parents (click to go there), its branches and tags (each a menu), the
     changed files with their status, and the diff of the chosen file: request changes
     (`SemanticDiff`) for files of the active owner, lines (`FileDiff`) for everything else
     and on request. Any repository: a file belongs to an owner only when the host says so
     (`GitContent.ownerOf`). -->
<script lang="ts">
  import type { ContentDiff, GitChangedFile, GitDiff, GitGraphCommit, GitRef } from "../types";
  import type { GitActionTarget } from "../extensions";
  import { gitActionEntries } from "../extension-git";
  import { getGitExtensions } from "../host";
  import ExtensionGitActions from "../ExtensionGitActions.svelte";
  import { fileOwnership, getGitHost } from "../host";
  import { Avatar, Badge, Button, Chip, Dropdown, EmptyState, Icon, IconButton, SegmentedControl, Skeleton, tooltip } from "@tailor-kit/ui";
  import FileEditorSwitch from "../extension/FileEditorSwitch.svelte";
  import { runAction, toMenu, type ActionHost } from "./actions";
  import FileDiff from "./FileDiff.svelte";
  import { absoluteTime, commitMenu, refLabels, refMenu, relativeTime, semanticFor, shortId, type GraphContext } from "./model";
  import RefLabel from "./RefLabel.svelte";

  let {
    commit,
    base = null,
    byFull,
    ctx,
    actions,
    now,
    onclearbase,
  }: {
    commit: GitGraphCommit;
    /** A second, marked commit: the details compare `base` → `commit`. */
    base?: GitGraphCommit | null;
    byFull: ReadonlyMap<string, GitRef>;
    ctx: GraphContext;
    actions: ActionHost;
    now: number;
    onclearbase: () => void;
  } = $props();

  const host = getGitHost();
  const extensions = getGitExtensions();
  const repository = $derived(host.activeRepository ?? "");
  const fail = (message: string) => host.notifyError(message);
  /** The side the shown file is compared against: the marked commit, else the first parent. */
  const diffFrom = $derived(base?.oid ?? commit.parents[0] ?? null);

  const STATUS = { added: "A", modified: "M", deleted: "D", renamed: "R" } as const;
  const labels = $derived(refLabels(commit.refs, byFull).filter((l) => l.ref.kind !== "head"));
  const from = $derived(base?.oid ?? null);

  let files = $state<GitChangedFile[] | null>(null);
  let filesError = $state<string | null>(null);
  let chosen = $state<string | null>(null);
  let mode = $state<"changes" | "lines">("changes");
  let lineDiff = $state<GitDiff | null>(null);
  let lineError = $state<string | null>(null);
  let semantic = $state<ContentDiff | null>(null);
  let semanticError = $state<string | null>(null);
  let semanticKey = "";
  let token = 0;

  const file = $derived(files?.find((f) => f.path === chosen) ?? null);
  const active = $derived(host.content?.activeOwner ?? null);
  const ownership = $derived(fileOwnership(host, file ? host.content?.ownerOf(file) : undefined));
  /** Request changes are shown for files of the active collection (the semantic diff reads it);
   *  a host without content shows lines only. */
  const semanticHere = $derived(Boolean(file && !file.binary && ownership.semantic));
  const otherOwner = $derived(ownership.other);
  const SemanticDiff = $derived(host.content?.components.SemanticDiff);
  const showSemantic = $derived(semanticHere && mode === "changes");

  $effect(() => {
    const to = commit.oid;
    const f = from;
    const mine = ++token;
    files = null;
    filesError = null;
    chosen = null;
    semantic = null;
    semanticKey = "";
    host.backend.git
      .changes(f, to)
      .then((list) => {
        if (mine !== token) return;
        files = list;
        chosen = list[0]?.path ?? null;
      })
      .catch((e: unknown) => {
        if (mine === token) filesError = e instanceof Error ? e.message : String(e);
      });
  });

  $effect(() => {
    const to = commit.oid;
    const f = from;
    const path = chosen;
    if (!path || showSemantic) return;
    const mine = token;
    lineDiff = null;
    lineError = null;
    host.backend.git
      .fileDiff(f, to, path)
      .then((d) => {
        if (mine === token && chosen === path) lineDiff = d;
      })
      .catch((e: unknown) => {
        if (mine === token && chosen === path) lineError = e instanceof Error ? e.message : String(e);
      });
  });

  $effect(() => {
    if (!showSemantic) return;
    const key = `${active}:${from ?? ""}:${commit.oid}`;
    if (key === semanticKey) return;
    semanticKey = key;
    semantic = null;
    semanticError = null;
    const to = commit.oid;
    const f = from;
    (f ? host.content!.diffs.between(f, to) : host.content!.diffs.commit(to))
      .then((d) => {
        if (semanticKey === key) semantic = d;
      })
      .catch((e: unknown) => {
        if (semanticKey === key) semanticError = e instanceof Error ? e.message : String(e);
      });
  });

  const fileSemantic = $derived(semantic && file ? semanticFor(semantic, file.path) : null);

  function split(path: string): { dir: string; name: string } {
    const p = path.replace(/^\//, "");
    const i = p.lastIndexOf("/");
    return i < 0 ? { dir: "", name: p } : { dir: p.slice(0, i + 1), name: p.slice(i + 1) };
  }

  function fileKey(e: KeyboardEvent) {
    if (!files || files.length === 0) return;
    const i = files.findIndex((f) => f.path === chosen);
    const next = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? files.length - 1 : null;
    if (next === null) return;
    e.preventDefault();
    const f = files[Math.max(0, Math.min(files.length - 1, next))]!;
    chosen = f.path;
    (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-path="${CSS.escape(f.path)}"]`)?.focus();
  }

  // Extensions' actions: on the commit (or on the two marked commits) at the end of its menu.
  const commitTarget = $derived<GitActionTarget>(
    base ? { kind: "commits", repository, commits: [commit.oid, base.oid] } : { kind: "commit", repository, commit: commit.oid },
  );
  const commitItems = $derived([
    ...toMenu(commitMenu(commit, refLabels(commit.refs, byFull), ctx), actions),
    ...gitActionEntries(extensions, commitTarget, fail),
  ]);
  const refItems = (ref: GitRef) => [
    ...toMenu(refMenu(ref, ctx), actions),
    ...gitActionEntries(extensions, { kind: "ref", repository, ref: ref.full }, fail),
  ];
</script>

{#snippet diffBody()}
        {#if showSemantic}
          {#if semanticError}
            <p class="error" role="alert">{semanticError}</p>
          {:else if !fileSemantic}
            <Skeleton lines={4} label="Loading the request changes" />
          {:else if fileSemantic.changes.length === 0 && fileSemantic.files.length === 0}
            <EmptyState size="sm" icon="check" title="No request changes" text="Only formatting or comments changed. Switch to Lines to see them." />
          {:else}
            <div data-testid="graph-semantic-diff">{#if SemanticDiff}<SemanticDiff diff={fileSemantic} />{/if}</div>
          {/if}
        {:else}
          <FileDiff diff={lineDiff} error={lineError} />
        {/if}
{/snippet}

<section class="details" aria-label="Commit details" data-testid="graph-details">
  {#if base}
    <div class="comparing" data-testid="graph-comparing">
      <Icon name="compare" size="sm" />
      <span>Changes from <code>{shortId(base.oid)}</code> to <code>{shortId(commit.oid)}</code></span>
      <span class="spacer"></span>
      <Button size="sm" variant="secondary" data-testid="graph-open-compare" onclick={() => actions.views.open({ kind: "compare", from: base.oid, to: commit.oid })}>Run both</Button>
      <IconButton icon="close" size="sm" label="Stop comparing" onclick={onclearbase} />
    </div>
  {/if}

  <header class="head">
    <h2 class="summary" data-testid="graph-details-summary">{commit.summary}</h2>
    <ExtensionGitActions target={commitTarget} label="Commit actions" />
    <Dropdown items={commitItems} menuLabel="Commit actions" placement="bottom-end">
      {#snippet trigger(props)}
        <IconButton icon="more" label="Commit actions" size="sm" {...props} data-testid="graph-details-menu" />
      {/snippet}
    </Dropdown>
  </header>

  <div class="meta">
    <Avatar name={commit.author} />
    <div class="who">
      <span class="author">{commit.author}</span>
      <span class="email">{commit.email}</span>
    </div>
    <span class="when" use:tooltip={absoluteTime(commit.time)}>{relativeTime(commit.time, now)}</span>
  </div>

  <dl class="facts">
    <dt>Date</dt>
    <dd>{absoluteTime(commit.time)}</dd>
    <dt>Commit</dt>
    <dd class="id">
      <code data-testid="graph-details-oid">{commit.oid}</code>
      <IconButton icon="copy" size="sm" label="Copy commit id" onclick={() => runAction(actions, { kind: "copy", text: commit.oid })} />
    </dd>
    {#if commit.parents.length > 0}
      <dt>{commit.parents.length > 1 ? "Parents" : "Parent"}</dt>
      <dd class="chips">
        {#each commit.parents as p, i (p)}
          <Chip
            label={shortId(p)}
            mono
            size="sm"
            icon="commit"
            data-testid="graph-parent"
            title={i === 0 ? "Go to the commit before this one" : "Go to the merged commit"}
            onclick={() => actions.select(p)}
          />
        {/each}
        {#if commit.parents.length > 1}<Badge value="Merge" tone="info" />{/if}
      </dd>
    {/if}
    {#if labels.length > 0}
      <dt>At</dt>
      <dd class="chips">
        {#each labels as l (l.ref.full)}
          <Dropdown items={refItems(l.ref)} menuLabel="{l.ref.name} actions">
            {#snippet trigger(props)}<RefLabel label={l} size="sm" trigger={props} />{/snippet}
          </Dropdown>
        {/each}
      </dd>
    {/if}
  </dl>

  <div class="files-head">
    <h3>Changed files</h3>
    {#if files}<Badge kind="count" value={files.length} />{/if}
  </div>

  {#if filesError}
    <p class="error" role="alert">{filesError}</p>
  {:else if !files}
    <Skeleton lines={3} label="Loading the changed files" />
  {:else if files.length === 0}
    <EmptyState size="sm" icon="check" title="No file changes" text={base ? "Both commits have the same files." : "This commit changes no files (for example a merge that took one side)."} />
  {:else}
    <ul class="files" role="listbox" aria-label="Changed files" tabindex="-1" onkeydown={fileKey} data-testid="graph-files">
      {#each files as f (f.path)}
        {@const parts = split(f.path)}
        <li>
          <button
            type="button"
            role="option"
            aria-selected={f.path === chosen}
            class="file"
            class:chosen={f.path === chosen}
            data-testid="graph-file"
            data-path={f.path}
            tabindex={f.path === chosen ? 0 : -1}
            onclick={() => (chosen = f.path)}
          >
            <Badge kind="status" value={STATUS[f.change]} />
            <span class="path" title={f.from ? `${f.from} → ${f.path}` : f.path}>
              {#if f.from}<span class="dir">{f.from.replace(/^\//, "")} → </span>{/if}<span class="dir">{parts.dir}</span><span class="name">{parts.name}</span>
            </span>
            {#if host.content?.ownerOf(f)}<span class="tag" use:tooltip={host.content.texts.ownedFileTitle}><Icon name="library" size="sm" /></span>{/if}
            {#if f.binary}<Badge value="binary" />{/if}
          </button>
        </li>
      {/each}
    </ul>

    {#if file}
      <div class="diff-head">
        <span class="diff-title">{split(file.path).name}</span>
        <span class="spacer"></span>
        {#if !file.binary && file.change !== "deleted"}
          <Button size="sm" variant="ghost" icon="file-text" data-testid="graph-file-blame" onclick={() => actions.views.open({ kind: "blame", path: file.path, rev: commit.oid })}>Blame</Button>
        {/if}
        <Button size="sm" variant="ghost" icon="history" data-testid="graph-file-history" onclick={() => actions.views.open({ kind: "file-history", path: file.path })}>File history</Button>
        <ExtensionGitActions target={{ kind: "file", repository, path: file.path, rev: commit.oid }} label="File actions" />
        {#if semanticHere}
          <SegmentedControl
            size="sm"
            label="How to show the change"
            bind:value={mode}
            options={[
              { value: "changes", label: "Requests" },
              { value: "lines", label: "Lines" },
            ]}
            data-testid="graph-diff-mode"
          />
        {/if}
      </div>
      {#if otherOwner}
        <div class="note">
          <Icon name="info" size="sm" />
          <span>{host.content?.texts.otherOwnerBefore}<strong>{otherOwner.name}</strong>{host.content?.texts.otherOwnerAfter}</span>
          <Button size="sm" variant="ghost" onclick={() => void host.content?.activate(otherOwner.id)}>Make active</Button>
        </div>
      {/if}
      <div class="diff" data-testid="graph-diff">
        {#if diffFrom && !showSemantic && !file.binary}
          <FileEditorSwitch path={file.path} from={diffFrom} to={commit.oid}>{@render diffBody()}</FileEditorSwitch>
        {:else}
          {@render diffBody()}
        {/if}
      </div>
    {/if}
  {/if}
</section>

<style>
  .details {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
    padding: var(--rt-space-6);
    min-width: 0;
    box-sizing: border-box;
  }
  .comparing {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    padding: var(--rt-space-3) var(--rt-space-4);
    border: 1px solid var(--rt-info-border);
    border-radius: var(--rt-radius-md);
    background: var(--rt-info-bg);
    color: var(--rt-info-fg);
    font: var(--rt-font-small);
  }
  .comparing code,
  .id code {
    font-family: var(--rt-font-mono);
  }
  .spacer {
    flex: 1;
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: var(--rt-space-3);
  }
  .summary {
    flex: 1;
    margin: 0;
    font: var(--rt-font-heading);
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .author {
    font: var(--rt-font-body);
    font-weight: var(--rt-weight-semibold);
  }
  .email,
  .when {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .facts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--rt-space-3) var(--rt-space-5);
    margin: 0;
    font: var(--rt-font-small);
  }
  dt {
    color: var(--rt-fg-muted);
    line-height: var(--rt-control-sm);
  }
  dd {
    margin: 0;
    min-width: 0;
    display: flex;
    align-items: center;
    line-height: var(--rt-control-sm);
  }
  .id code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: all;
  }
  .chips {
    flex-wrap: wrap;
    gap: var(--rt-space-2);
  }
  .files-head {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    border-top: 1px solid var(--rt-border);
    padding-top: var(--rt-space-5);
  }
  h3 {
    margin: 0;
    font: var(--rt-font-caption);
    font-weight: var(--rt-weight-semibold);
    color: var(--rt-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    max-height: 30vh;
    overflow: auto;
  }
  .file {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    width: 100%;
    min-height: var(--rt-control-sm);
    padding: 0 var(--rt-space-3);
    border: 0;
    border-radius: var(--rt-radius-sm);
    background: transparent;
    color: var(--rt-fg);
    font: var(--rt-font-small);
    text-align: left;
    cursor: pointer;
  }
  .file:hover {
    background: var(--rt-bg-hover);
  }
  .file.chosen {
    background: var(--rt-accent-subtle);
    color: var(--rt-accent-subtle-fg);
  }
  .file:focus-visible {
    outline: none;
    box-shadow: var(--rt-focus-ring);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: ltr;
  }
  .dir {
    color: var(--rt-fg-muted);
  }
  .chosen .dir {
    color: inherit;
    opacity: 0.8;
  }
  .name {
    font-weight: var(--rt-weight-medium);
  }
  .tag {
    display: inline-flex;
    color: var(--rt-fg-muted);
  }
  .diff-head {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    min-height: var(--rt-control-sm);
  }
  .diff-title {
    font: var(--rt-font-small);
    font-weight: var(--rt-weight-semibold);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    padding: var(--rt-space-3) var(--rt-space-4);
    border-radius: var(--rt-radius-md);
    background: var(--rt-info-bg);
    color: var(--rt-info-fg);
    font: var(--rt-font-small);
  }
  .diff {
    min-width: 0;
  }
  .error {
    margin: 0;
    color: var(--rt-danger-fg);
    font: var(--rt-font-small);
  }
</style>

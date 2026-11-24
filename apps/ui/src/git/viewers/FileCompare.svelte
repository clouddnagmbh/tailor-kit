<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Two versions of one file (wave 15, strand B): loads both through `git.blob` and shows them
     the way their content wants: text with words marked, images side by side / faded / as a
     difference, anything else as metadata (binary, Git LFS pointer, too large). `null` for a
     side means the file does not exist there (new or deleted); a `rev` of `null` is the
     working tree. -->
<script lang="ts">
  import type { GitBlob } from "../../commands/types";
  import { getApp } from "../../state/app.svelte";
  import { EmptyState, Spinner } from "../../ui";
  import ImageDiff from "./ImageDiff.svelte";
  import MetaDiff from "./MetaDiff.svelte";
  import { blobKind, type Side } from "./model";
  import TextDiff from "./TextDiff.svelte";

  let {
    before,
    after,
    reload = 0,
    onhunks,
  }: { before: Side | null; after: Side | null; reload?: number; onhunks?: (count: number) => void } = $props();
  const app = getApp();

  let blobs = $state<{ old: GitBlob | null; new: GitBlob | null } | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const [b, a] = [before, after];
    void reload;
    blobs = null;
    error = null;
    const git = app.backend.git;
    Promise.all([b ? git.blob(b.rev, b.path) : null, a ? git.blob(a.rev, a.path) : null]).then(
      ([old, next]) => {
        if (b === before && a === after) blobs = { old, new: next };
      },
      (e: unknown) => {
        if (b === before && a === after) error = e instanceof Error ? e.message : String(e);
      },
    );
  });

  type Shown = "text" | "image" | "meta";
  const shown = $derived.by((): Shown | null => {
    if (!blobs) return null;
    const kinds = [blobs.old, blobs.new].filter((b): b is GitBlob => b !== null).map(blobKind);
    if (kinds.every((k) => k === "image")) return "image";
    if (kinds.every((k) => k === "text")) return "text";
    return "meta";
  });
</script>

<div class="compare" data-testid="file-compare" data-shown={shown ?? "loading"}>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !blobs}
    <p class="hint"><Spinner size="sm" label="Loading" /> Loading…</p>
  {:else if !blobs.old && !blobs.new}
    <EmptyState icon="file" title="Nothing to show" text="The file is on neither side." size="sm" />
  {:else if shown === "image"}
    <ImageDiff before={blobs.old} after={blobs.new} />
  {:else if shown === "text"}
    <TextDiff before={blobs.old?.text ?? null} after={blobs.new?.text ?? null} {onhunks} />
  {:else}
    <MetaDiff before={blobs.old} after={blobs.new} />
  {/if}
</div>

<style>
  .hint,
  .error {
    display: flex;
    align-items: center;
    gap: var(--rt-space-3);
    margin: 0;
    padding: var(--rt-space-5);
  }
  .hint {
    color: var(--rt-fg-muted);
  }
  .error {
    color: var(--rt-danger-fg);
  }
</style>

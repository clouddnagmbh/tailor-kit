<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- One file as it is (wave 15, strand B): text with line numbers, an image, or the facts of a
     binary file, a Git LFS pointer or a file over the limit. -->
<script lang="ts">
  import type { GitBlob } from "../../commands/types";
  import { Badge } from "../../ui";
  import { blobKind, dataUrl, formatSize, parseLfsPointer, shortId } from "./model";

  let { blob }: { blob: GitBlob } = $props();

  const LINE_LIMIT = 5000;
  const kind = $derived(blobKind(blob));
  const lines = $derived(blob.text ? blob.text.replace(/\r?\n$/, "").split(/\r?\n/) : []);
  const lfs = $derived(parseLfsPointer(blob.text));
</script>

<div class="blob" data-testid="blob-view" data-kind={kind}>
  {#if kind === "text"}
    <ol class="code" data-testid="blob-lines">
      {#each lines.slice(0, LINE_LIMIT) as line, i (i)}
        <li><span class="no" aria-hidden="true">{i + 1}</span><span class="text">{line}</span></li>
      {/each}
    </ol>
    {#if lines.length > LINE_LIMIT}
      <p class="note" data-testid="blob-more">{lines.length - LINE_LIMIT} more lines are not shown.</p>
    {/if}
    {#if lines.length === 0}<p class="note">The file is empty.</p>{/if}
  {:else if kind === "image"}
    <div class="frame"><img src={dataUrl(blob)} alt={blob.path} data-testid="blob-image" /></div>
    <p class="note">{blob.media_type} · {formatSize(blob.size)}</p>
  {:else}
    <div class="facts" data-testid="blob-facts">
      <p class="lead">
        <Badge value={kind === "lfs" ? "Git LFS pointer" : kind === "large" ? "Too large to show" : "Binary file"} tone={kind === "lfs" ? "info" : "neutral"} />
        <span class="note">{formatSize(blob.size)}{blob.media_type ? ` · ${blob.media_type}` : ""}</span>
      </p>
      {#if lfs}
        <dl>
          <dt>LFS object</dt>
          <dd>{lfs.oid}</dd>
          {#if lfs.size !== null}<dt>Real size</dt><dd>{formatSize(lfs.size)}</dd>{/if}
        </dl>
        <p class="note">The repository holds this pointer; the file itself is fetched by Git LFS.</p>
      {/if}
      {#if blob.oid}<p class="note">Blob {shortId(blob.oid)}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .code {
    margin: 0;
    padding: var(--rt-space-3) 0;
    list-style: none;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
    line-height: var(--rt-leading-small);
  }
  .code li {
    display: grid;
    grid-template-columns: 6ch minmax(0, 1fr);
    padding-right: var(--rt-space-5);
  }
  .no {
    padding-right: var(--rt-space-3);
    text-align: right;
    color: var(--rt-fg-muted);
    user-select: none;
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .note {
    margin: 0;
    padding: var(--rt-space-3) var(--rt-space-5);
    color: var(--rt-fg-muted);
    font-size: var(--rt-text-small);
  }
  .frame {
    display: flex;
    justify-content: center;
    margin: var(--rt-space-5);
    padding: var(--rt-space-5);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-md);
    background: repeating-conic-gradient(var(--rt-surface-sunken) 0% 25%, var(--rt-surface-2) 0% 50%) 50% / 16px 16px;
  }
  img {
    max-width: 100%;
    height: auto;
  }
  .facts {
    padding: var(--rt-space-5);
  }
  .lead {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    margin: 0;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--rt-space-2) var(--rt-space-5);
    margin: var(--rt-space-4) 0;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
  }
  dt {
    color: var(--rt-fg-muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>

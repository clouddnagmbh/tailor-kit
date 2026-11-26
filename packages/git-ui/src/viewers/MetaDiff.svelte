<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- What is known about a file that has no lines to show (wave 15, strand B): a binary file, a
     Git LFS pointer, a file over the viewer limit. Old and new side by side as facts: size,
     type, id, and for a pointer the id and size of the real file. -->
<script lang="ts">
  import type { GitBlob } from "../types";
  import { Badge } from "@tailor-kit/ui";
  import { blobKind, formatSize, parseLfsPointer, shortId, sizeChange } from "./model";

  let { before, after }: { before: GitBlob | null; after: GitBlob | null } = $props();

  const kind = $derived(blobKind((after ?? before)!));
  const lfs = (b: GitBlob | null) => (b?.lfs_pointer ? parseLfsPointer(b.text) : null);
  const label = $derived(
    kind === "lfs" ? "Git LFS pointer" : kind === "large" ? "Too large to show" : kind === "image" ? "Image" : "Binary file",
  );

  interface Fact {
    name: string;
    old: string;
    new: string;
  }
  const facts = $derived.by((): Fact[] => {
    const out: Fact[] = [
      { name: "Size", old: before ? formatSize(before.size) : "", new: after ? formatSize(after.size) : "" },
      { name: "Type", old: before?.media_type ?? "", new: after?.media_type ?? "" },
      { name: "Blob", old: shortId(before?.oid), new: shortId(after?.oid) },
    ];
    const [a, b] = [lfs(before), lfs(after)];
    if (a || b) {
      out.push(
        { name: "LFS object", old: shortId(a?.oid, 16), new: shortId(b?.oid, 16) },
        { name: "LFS size", old: a?.size == null ? "" : formatSize(a.size), new: b?.size == null ? "" : formatSize(b.size) },
      );
    }
    return out.filter((f) => f.old !== "" || f.new !== "");
  });
</script>

<div class="meta" data-testid="meta-diff" data-kind={kind}>
  <p class="lead">
    <Badge value={label} tone={kind === "lfs" ? "info" : "neutral"} />
    <span>No lines to compare.</span>
    {#if before && after}<span class="muted" data-testid="meta-delta">{sizeChange(before.size, after.size)}</span>{/if}
  </p>
  {#if kind === "lfs"}
    <p class="note">The repository holds a pointer, not the file itself. Fetch the real file with Git LFS to see it.</p>
  {/if}
  <table>
    <thead>
      <tr><th scope="col"></th><th scope="col">{before ? (after ? "Before" : "Deleted") : ""}</th><th scope="col">{after ? (before ? "After" : "New") : ""}</th></tr>
    </thead>
    <tbody>
      {#each facts as f (f.name)}
        <tr data-testid="meta-row" data-name={f.name}><th scope="row">{f.name}</th><td>{f.old}</td><td>{f.new}</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .meta {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
    padding: var(--rt-space-5);
  }
  .lead {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    margin: 0;
  }
  .note {
    margin: 0;
    color: var(--rt-fg-muted);
  }
  .muted {
    color: var(--rt-fg-muted);
  }
  table {
    border-collapse: collapse;
    font-size: var(--rt-text-small);
    align-self: flex-start;
  }
  th,
  td {
    padding: var(--rt-space-2) var(--rt-space-5);
    text-align: left;
    border-bottom: 1px solid var(--rt-border);
    font-family: var(--rt-font-mono);
  }
  th {
    color: var(--rt-fg-muted);
    font-weight: var(--rt-weight-semibold);
    font-family: inherit;
  }
</style>

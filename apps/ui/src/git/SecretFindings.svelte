<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Credentials the secret scanner found in the files of a commit (wave 5, strand E): file,
     line, kind, masked snippet and hint for each. The commit does not happen while there are
     findings; there is no "commit anyway". A line that is not a credential is marked with
     `# rt-scan:allow` in the file. -->
<script lang="ts">
  import type { SecretFinding } from "../commands/types";

  let { findings }: { findings: SecretFinding[] } = $props();

  const files = $derived(new Set(findings.map((f) => f.path)).size);
</script>

<section class="findings" role="alert" aria-labelledby="git-secrets-title" data-testid="git-secrets">
  <h3 id="git-secrets-title">
    Commit blocked: {findings.length}
    {findings.length === 1 ? "credential" : "credentials"} in {files}
    {files === 1 ? "file" : "files"}
  </h3>
  <ul>
    {#each findings as f, i (i)}
      <li data-testid="git-secret-finding">
        <div class="where">
          <span class="location" data-testid="git-secret-location">{f.path}:{f.line}</span>
          <span class="kind" data-testid="git-secret-kind">{f.kind}</span>
        </div>
        <code class="snippet" data-testid="git-secret-snippet">{f.snippet}</code>
        <p class="hint" data-testid="git-secret-hint">{f.hint}</p>
      </li>
    {/each}
  </ul>
  <p class="next">Fix the files and save, then commit again.</p>
</section>

<style>
  .findings {
    margin-top: 6px;
    padding: 6px var(--rt-gap);
    border: 1px solid var(--rt-danger);
    border-radius: var(--rt-radius);
  }
  h3 {
    margin: 0 0 4px;
    font-size: inherit;
    color: var(--rt-danger);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    padding: 4px 0;
    border-top: 1px solid var(--rt-border);
  }
  li:first-child {
    border-top: 0;
  }
  .where {
    display: flex;
    gap: var(--rt-gap);
    justify-content: space-between;
  }
  .location {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--rt-font-mono);
  }
  .kind {
    flex-shrink: 0;
    color: var(--rt-fg-muted);
    font-size: var(--rt-size-small);
  }
  .snippet {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
  }
  .hint,
  .next {
    margin: 2px 0 0;
    font-size: var(--rt-size-small);
  }
  .next {
    color: var(--rt-fg-muted);
  }
</style>

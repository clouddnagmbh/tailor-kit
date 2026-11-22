<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The assistant's Markdown, as text nodes only (`markdown.ts`). Wave 8, strand E. -->
<script lang="ts">
  import { parseMarkdown, type Span } from "./markdown";

  let { text, testid }: { text: string; testid?: string } = $props();
  const blocks = $derived(parseMarkdown(text));
</script>

{#snippet spans(list: Span[])}
  {#each list as s, i (i)}
    {#if s.code}<code>{s.text}</code>{:else if s.bold}<strong>{s.text}</strong>{:else if s.italic}<em>{s.text}</em>{:else}{s.text}{/if}
  {/each}
{/snippet}

<div class="md" data-testid={testid}>
  {#each blocks as b, i (i)}
    {#if b.kind === "p"}
      <p>{@render spans(b.spans)}</p>
    {:else if b.kind === "h"}
      <p class="h">{@render spans(b.spans)}</p>
    {:else if b.kind === "code"}
      <pre><code>{b.text}</code></pre>
    {:else if b.kind === "ul"}
      <ul>
        {#each b.items as item, j (j)}<li>{@render spans(item)}</li>{/each}
      </ul>
    {:else}
      <ol>
        {#each b.items as item, j (j)}<li>{@render spans(item)}</li>{/each}
      </ol>
    {/if}
  {/each}
</div>

<style>
  .md {
    display: flex;
    flex-direction: column;
    gap: 6px;
    line-height: 1.45;
  }
  p,
  ul,
  ol,
  pre {
    margin: 0;
  }
  ul,
  ol {
    padding-left: 1.4em;
  }
  .h {
    font-weight: 600;
  }
  code {
    font-family: var(--rt-font-mono);
    font-size: var(--rt-size-small);
  }
  pre {
    padding: 6px 8px;
    overflow: auto;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius);
    background: var(--rt-bg-panel);
  }
</style>

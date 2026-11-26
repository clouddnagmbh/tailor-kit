<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- "What will be sent" (wave 16): the exact, masked call before it leaves, readable: where it
     goes, how much, how many secret values were replaced (those places are marked), the
     instructions, the messages and the tools. "Show the raw call" shows the JSON. Send or
     Cancel; Escape cancels and nothing is sent. The result is `{ send, remember }`. -->
<script lang="ts">
  import { Badge, Button, Checkbox, DialogFooter, Icon } from "@tailor-kit/ui";

  import { segments } from "./preview";
  import type { AiPreview } from "./types";

  /** What a dialog hosted by the product gets (`ExtensionDialogProps` of RequestTailor, `GitDialogProps`). */
  let { close, props }: { close: (result?: unknown) => void; props: Record<string, unknown> } = $props();
  const preview = $derived(props.preview as AiPreview);
  let raw = $state(false);
  let remember = $state(false);
  const host = $derived(preview.endpoint.replace(/^[a-z]+:\/\//, "").split("/")[0] ?? "");
  const kb = $derived(preview.bytes < 1024 ? `${preview.bytes} bytes` : `${(preview.bytes / 1024).toFixed(1)} KB`);
  const result = (send: boolean) => close({ send, remember: send && remember });
</script>

<div class="preview" data-testid="ai-preview">
  <div class="summary">
    <p data-testid="ai-preview-target">
      To <strong>{host}</strong> · {preview.model} · profile <strong>{preview.profile}</strong>
    </p>
    <div class="facts">
      <Badge kind="label" value={kb} />
      <Badge
        kind="label"
        tone={preview.masked === 0 ? "neutral" : "warn"}
        data-testid="ai-preview-masked"
        value={preview.masked === 0 ? "no secret values" : `${preview.masked} secret value${preview.masked === 1 ? "" : "s"} replaced by ***`}
      />
    </div>
    <p class="muted">Only this leaves the app. Nothing is saved by the assistant; its answer is a proposal you can accept or discard.</p>
  </div>

  {#if !raw}
    <div class="blocks" data-testid="ai-preview-readable">
      {#snippet text(value: string)}
        <pre>{#each segments(value) as s}{#if s.masked}<mark title="A secret value was replaced here">***</mark>{:else}{s.text}{/if}{/each}</pre>
      {/snippet}
      <section>
        <h4>Instructions</h4>
        {@render text(preview.call.system)}
      </section>
      {#each preview.call.messages as m}
        <section>
          <h4>{m.role === "user" ? "You" : "Assistant"}</h4>
          {#if m.content}{@render text(m.content)}{/if}
          {#each m.tool_calls ?? [] as c}<p class="muted"><Icon name="code" size="sm" /> calls {c.name}</p>{/each}
          {#each m.tool_results ?? [] as r}{@render text(r.content)}{/each}
        </section>
      {/each}
      {#if (preview.call.tools ?? []).length > 0}
        <section>
          <h4>Tools the model may call</h4>
          <p class="muted">{preview.call.tools!.map((t) => t.name).join(", ")}</p>
        </section>
      {/if}
    </div>
  {:else}
    <pre class="rawjson" data-testid="ai-preview-raw">{JSON.stringify(preview.call, null, 2)}</pre>
  {/if}

  <DialogFooter>
    {#snippet start()}
      <Button variant="ghost" size="sm" data-testid="ai-preview-toggle" onclick={() => (raw = !raw)}>{raw ? "Show readable" : "Show the raw call"}</Button>
      {#if preview.required}
        <Checkbox label="Don't show this automatically" bind:checked={remember} data-testid="ai-preview-remember" />
      {/if}
    {/snippet}
    <Button data-testid="ai-preview-cancel" onclick={() => result(false)}>Cancel</Button>
    <Button variant="primary" data-testid="ai-preview-send" onclick={() => result(true)}>Send</Button>
  </DialogFooter>
</div>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
  }
  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rt-space-3);
  }
  .blocks {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-4);
    max-height: 45vh;
    overflow: auto;
  }
  h4 {
    margin: 0 0 var(--rt-space-2);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    text-transform: uppercase;
  }
  pre {
    margin: 0;
    padding: var(--rt-space-4) var(--rt-space-5);
    overflow: auto;
    font: var(--rt-font-small);
    font-family: var(--rt-font-mono);
    white-space: pre-wrap;
    word-break: break-word;
    background: var(--rt-surface-2);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-md);
  }
  .rawjson {
    max-height: 45vh;
  }
  mark {
    padding: 0 var(--rt-space-2);
    color: var(--rt-warn-fg);
    background: var(--rt-warn-bg);
    border: 1px solid var(--rt-warn-border);
    border-radius: var(--rt-radius-sm);
  }
</style>

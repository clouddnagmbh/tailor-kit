<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- "What will be sent" (wave 16 interface): the exact, masked call before it leaves. Interface
     stub: provider, endpoint, model and the call as JSON, Send and Cancel. Strand A designs it
     (messages readable, masked places marked, "don't show again", Escape). -->
<script lang="ts">
  import type { AiPreview } from "../commands/types";
  import type { ExtensionDialogProps } from "../extensions";

  let { close, props }: ExtensionDialogProps = $props();
  const preview = $derived(props.preview as AiPreview);
</script>

<div class="preview" data-testid="ai-preview">
  <p>
    To <strong>{preview.endpoint}</strong> ({preview.model}), {preview.bytes} bytes,
    {preview.masked} secret value(s) masked.
  </p>
  <pre>{JSON.stringify(preview.call, null, 2)}</pre>
  <div class="actions">
    <button onclick={() => close(false)}>Cancel</button>
    <button class="primary" onclick={() => close(true)}>Send</button>
  </div>
</div>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px 16px;
  }
  pre {
    max-height: 50vh;
    overflow: auto;
    margin: 0;
    padding: 8px;
    background: var(--rt-bg-subtle, transparent);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>

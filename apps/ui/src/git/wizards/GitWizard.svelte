<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 14, strand G: one Git wizard in the dialog slot: form, plan in plain words (what moves,
     which files change, merge preview, backup), confirm, result with "Undo". Placeholder until
     strand G: it only says so and closes. -->
<script lang="ts">
  import type { ExtensionDialogProps } from "../../extensions";
  import type { GitWizardRequest } from "./open";

  let { close, props }: ExtensionDialogProps = $props();
  const request = $derived(props as unknown as { request: GitWizardRequest });
</script>

<p data-testid="git-wizard" data-kind={request.request.kind}>This Git assistant arrives with wave 14.</p>
<button class="button" onclick={() => close()}>Close</button>

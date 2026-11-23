<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- "Clone repository" tile of the New… dialog (wave 15, strand A): the clone form in the shell's
     dialog frame (`AppState.showDialog`). The clone joins the sidebar. -->
<script lang="ts">
  import type { ExtensionDialogProps } from "../extensions";
  import CloneForm from "./CloneForm.svelte";

  let { ctx, close }: ExtensionDialogProps = $props();
</script>

<CloneForm
  oncancel={() => close()}
  ondone={async (entry) => {
    close(entry);
    await ctx.app.adoptRepository(entry);
  }}
/>

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- "Clone repository" tile of the New… dialog (wave 15, strand A): the clone form in the shell's
     dialog frame (`AppState.showDialog`). The clone joins the sidebar. -->
<script lang="ts">
  import { getGitHost, type GitDialogProps } from "./host";
  import CloneForm from "./CloneForm.svelte";

  let { close }: GitDialogProps = $props();
  const host = getGitHost();
</script>

<CloneForm
  oncancel={() => close()}
  ondone={async (entry) => {
    close(entry);
    await host.adoptRepository(entry);
  }}
/>

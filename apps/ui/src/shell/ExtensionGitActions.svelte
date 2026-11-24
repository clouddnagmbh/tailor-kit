<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The actions extensions add to a thing in the Git views (wave 15, strand E): a commit, a
     selection of commits, a branch, a file, the changed files, a parked change, a conflict.
     At most two `inline` actions are buttons, the rest sit in the "More" menu
     (`ExtensionActions`); renders nothing without any, so a build without extensions looks as
     before. -->
<script lang="ts">
  import type { GitActionTarget } from "../extensions";
  import { extensionGitActions } from "./extension-git";
  import { getExtensions } from "./extension-tabs";
  import ExtensionActions from "./ExtensionActions.svelte";

  let {
    target,
    label = "Actions",
    size = "sm",
    variant = "secondary",
  }: {
    target: GitActionTarget;
    /** Accessible name of the group, e.g. "Commit actions". */
    label?: string;
    size?: "sm" | "md";
    variant?: "secondary" | "ghost";
  } = $props();

  const provided = getExtensions();
  const actions = $derived(extensionGitActions(provided, target));
</script>

<ExtensionActions {actions} {label} {size} {variant} />

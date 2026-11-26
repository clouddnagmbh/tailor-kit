<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The actions extensions add to a place (wave 16, strand C): on a request, a response, a run or
     bisect, a range of two revisions. The component itself moved to `packages/git-ui` (step K3);
     this wrapper keeps the old path and sends an action's error to the app's error line. -->
<script lang="ts">
  import Actions from "@tailor-kit/git-ui/ExtensionActions.svelte";
  import { getApp } from "../state/app.svelte";
  import type { ExtensionActionRef } from "./extension-actions";
  import type { ExtensionAction } from "../extensions";

  let {
    actions,
    label,
    size = "sm",
    variant = "secondary",
    collapse = false,
  }: {
    actions: ExtensionActionRef<ExtensionAction>[];
    /** Accessible name of the group, e.g. "Request actions". */
    label: string;
    size?: "sm" | "md";
    variant?: "secondary" | "ghost";
    /** In a narrow window the inline actions shrink to icon buttons (the label is the tooltip). */
    collapse?: boolean;
  } = $props();

  const app = getApp();
</script>

<Actions {actions} {label} {size} {variant} {collapse} onerror={(message) => (app.error = message)} />

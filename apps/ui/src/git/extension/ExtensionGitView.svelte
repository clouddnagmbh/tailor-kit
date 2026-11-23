<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 15 (interface): an extension's Git view (`AppExtension.gitViews`), by key
     `<extension>/<view>`; strand E owns it (for instance a frame around the view). -->
<script lang="ts">
  import { EmptyState } from "../../ui";
  import { getExtensions } from "../../shell/extension-tabs";
  import { extensionGitView } from "../../shell/extension-git";
  import { getApp } from "../../state/app.svelte";

  let { id, props, onclose }: { id: string; props: Record<string, unknown>; onclose: () => void } = $props();

  const extensions = getExtensions();
  const app = getApp();
  const view = $derived(extensionGitView(extensions, id));
  const repository = $derived(app.activeRepository ?? "");
</script>

{#if view && extensions}
  {@const View = view.component}
  <View ctx={extensions.ctx} {repository} {props} close={onclose} />
{:else}
  <EmptyState icon="graph" title="This view is not available" text="The extension that offered it is not active." data-testid="git-view-missing" />
{/if}

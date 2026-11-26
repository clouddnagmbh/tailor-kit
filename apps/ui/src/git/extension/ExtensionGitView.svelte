<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- An extension's Git view (`AppExtension.gitViews`, wave 15, strand E), by key
     `<extension>/<view>`, in the Git host's frame: the host shows the title bar and the close
     button, this adds the repository and a safety net: a view that throws while it renders shows
     a calm error with a way back instead of a blank area, and one that is no longer offered (its
     extension is off) says so. The extension gets the active repository's id, its own `props`
     and `close`. -->
<script lang="ts">
  import { Banner, Button, EmptyState } from "../../ui";
  import { extensionGitView } from "../../shell/extension-git";
  import { getExtensions } from "../../shell/extension-tabs";
  import { getGitHost } from "../host";

  let { id, props, onclose }: { id: string; props: Record<string, unknown>; onclose: () => void } = $props();

  const extensions = getExtensions();
  const host = getGitHost();
  const view = $derived(extensionGitView(extensions, id));
  const repository = $derived(host.activeRepository ?? "");
</script>

{#if view && extensions}
  {@const View = view.component}
  <div class="frame" data-testid="git-extension-view" data-view={id}>
    <svelte:boundary
      onerror={(e) => console.warn(`${host.brand.productName}: the Git view ${id} failed:`, e)}
    >
      <View ctx={extensions.ctx} {repository} {props} close={onclose} />
      {#snippet failed(error, reset)}
        <div class="failed" data-testid="git-extension-view-failed">
          <Banner tone="danger" title="This view stopped working">
            {error instanceof Error ? error.message : String(error)}
            {#snippet actions()}
              <Button size="sm" onclick={reset}>Try again</Button>
              <Button size="sm" variant="ghost" onclick={onclose}>Close</Button>
            {/snippet}
          </Banner>
        </div>
      {/snippet}
    </svelte:boundary>
  </div>
{:else}
  <EmptyState icon="graph" title="This view is not available" text="The extension that offered it is not active." data-testid="git-view-missing" />
{/if}

<style>
  .frame {
    display: flex;
    flex-direction: column;
    min-height: 100%;
  }
  .failed {
    padding: var(--rt-space-5);
  }
</style>

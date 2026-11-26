<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- Shows the one dialog the app passes in, in the shared Dialog frame (@tailor-kit/ui/Dialog.svelte):
     title bar, close button, Escape, focus kept inside, light and dark. The app renders the
     content through the `content` snippet and can end it with `DialogFooter` for its buttons.
     Escape closes the dialog before anything behind it sees the key. -->
<script lang="ts" generics="D extends HostedDialog">
  import Dialog from "@tailor-kit/ui/Dialog.svelte";
  import type { Snippet } from "svelte";

  import type { HostedDialog } from "./dialog";

  let {
    dialog,
    onclose,
    content,
    testid,
  }: {
    /** The open dialog, or `null`/`undefined` for none. A new `seq` remounts the content. */
    dialog: D | null | undefined;
    onclose: () => unknown;
    content: Snippet<[D]>;
    testid?: string;
  } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && dialog) {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

{#if dialog}
  {#key dialog.seq}
    <Dialog title={dialog.title} size={dialog.width ?? "md"} {testid} closeOnBackdrop={false} {onclose}>
      {@render content(dialog)}
    </Dialog>
  {/key}
{/if}

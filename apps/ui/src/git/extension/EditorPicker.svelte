<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The choice between the app's own diff or merge view and the editors extensions offer for
     the file (wave 15, strand E). Nothing is shown without an editor, so a build without
     extensions looks as before. -->
<script lang="ts">
  import type { ExtensionFileEditor } from "../../extensions";
  import type { ExtensionActionRef } from "../../shell/extension-actions";
  import { SegmentedControl } from "../../ui";

  let {
    editors,
    value = $bindable(""),
    kind,
    builtin = "Standard",
    onchange,
  }: {
    editors: readonly ExtensionActionRef<ExtensionFileEditor>[];
    /** The chosen `<extension>/<editor>`, `""` for the app's own view. */
    value?: string;
    kind: ExtensionFileEditor["kind"];
    /** Label of the app's own view. */
    builtin?: string;
    onchange?: (value: string) => void;
  } = $props();
</script>

{#if editors.length > 0}
  <span data-testid="editor-picker" data-kind={kind}>
    <SegmentedControl
      size="sm"
      label={kind === "merge" ? "Merge editor" : "Diff editor"}
      bind:value
      {onchange}
      options={[
        { value: "", label: builtin, testid: "editor-builtin" },
        ...editors.map((e) => ({ value: e.key, label: e.action.title, testid: "editor-extension" })),
      ]}
    />
  </span>
{/if}

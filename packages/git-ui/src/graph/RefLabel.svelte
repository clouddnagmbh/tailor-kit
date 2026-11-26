<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Wave 14, strand F: one ref label of the commit graph (a branch, a remote branch, a tag or a
     detached HEAD), built on Chip. A local branch shows the remote branch folded into it
     (`main` + `origin/main` in step) and what it has to send and get. Not a control in rows (the
     row is); `interactive` makes it a menu trigger in the details. The tooltip says what the
     label means in plain words (`../glossary.ts`). -->
<script lang="ts">
  import { GLOSSARY } from "../glossary";
  import { Chip, Icon, tooltip } from "@tailor-kit/ui";
  import type { TriggerProps } from "@tailor-kit/ui";
  import type { RefLabel } from "./model";
  import { syncText } from "./model";

  let {
    label,
    size = "sm",
    trigger,
  }: {
    label: RefLabel;
    size?: "sm" | "md";
    /** Dropdown trigger attributes: the label becomes the menu's button. */
    trigger?: TriggerProps;
  } = $props();

  const ref = $derived(label.ref);
  const tone = $derived(ref.kind === "tag" ? "warn" : ref.kind === "head" ? "danger" : ref.kind === "remote" ? "info" : "neutral");
  const icon = $derived(ref.kind === "tag" ? "tag" : ref.kind === "head" ? "commit" : "branch");

  const help = $derived.by(() => {
    switch (ref.kind) {
      case "local":
        return `${ref.current ? `${GLOSSARY["current-branch"].label}: ${ref.name}. ${GLOSSARY["current-branch"].text}` : `${GLOSSARY.branch.label}: ${ref.name}.`} ${syncText(ref)}${label.tracking ? "; the server has the same commit." : "."}`;
      case "remote":
        return `${GLOSSARY["remote-branch"].label}: ${ref.name}. ${GLOSSARY["remote-branch"].text}`;
      case "tag":
        return `${GLOSSARY.tag.label}: ${ref.name}${ref.message ? ` (${ref.message})` : ""}. ${GLOSSARY.tag.text}`;
      case "head":
        return `${GLOSSARY.detached.label}. ${GLOSSARY.detached.text}`;
    }
  });
</script>

<span
  class="ref"
  class:current={ref.current && ref.kind === "local"}
  data-testid="graph-ref"
  data-ref={ref.full}
  data-kind={ref.kind}
  use:tooltip={{ text: help, placement: "bottom" }}
>
  <span class="chip"><Chip
    label={ref.kind === "head" ? "HEAD" : ref.name}
    {icon}
    tone={ref.current && ref.kind === "local" ? "accent" : tone}
    selected={ref.current && ref.kind === "local"}
    {size}
    caret={Boolean(trigger)}
    {...trigger ?? {}}
  /></span>
  {#if label.tracking}
    <span class="tracking" data-testid="graph-ref-tracking" aria-label="also {label.tracking.name}"><Icon name="sync" size="sm" /></span>
  {/if}
  {#if ref.kind === "local" && ref.upstream && (ref.ahead > 0 || ref.behind > 0)}
    <span class="sync" data-testid="graph-ref-sync" aria-label={syncText(ref)}>
      {#if ref.ahead > 0}<span class="ahead">↑{ref.ahead}</span>{/if}
      {#if ref.behind > 0}<span class="behind">↓{ref.behind}</span>{/if}
    </span>
  {/if}
</span>

<style>
  .ref {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: var(--rt-space-1);
  }
  .chip {
    display: inline-flex;
    min-width: 0;
    max-width: 180px;
  }
  .tracking {
    display: inline-flex;
    color: var(--rt-info-fg);
  }
  .sync {
    display: inline-flex;
    gap: var(--rt-space-1);
    font: var(--rt-weight-semibold) var(--rt-text-caption) / 1 var(--rt-font-ui);
    font-variant-numeric: tabular-nums;
  }
  .ahead {
    color: var(--rt-ok-fg);
  }
  .behind {
    color: var(--rt-warn-fg);
  }
</style>

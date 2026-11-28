<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- How much a file or folder changed lately (wave 8, strand E; V9): a short bar after its name,
     the length the number of changes, the colour how long ago the last one was. The sentence is
     the tooltip and the accessible text. Raw numbers, no rating. -->
<script lang="ts">
  import type { FileActivity } from "../host";
  import { ageColor, ageStep } from "./color";

  let { activity, now = Date.now() / 1000 }: { activity: FileActivity; now?: number } = $props();

  const ago = $derived(Math.max(0, Math.floor((now - activity.last) / 86_400)));
  const sentence = $derived(
    `${activity.changes} ${activity.changes === 1 ? "change" : "changes"} in ${activity.days} days, the last ${ago === 0 ? "today" : ago === 1 ? "1 day ago" : `${ago} days ago`}`,
  );
</script>

<span class="activity" data-testid="file-activity" data-changes={activity.changes} data-step={ageStep(ago)} title={sentence}>
  <span class="track" aria-hidden="true"><span class="bar" style:width="{Math.max(8, activity.share * 100)}%" style:background={ageColor(ageStep(ago))}></span></span>
  <span class="sr-only">{sentence}</span>
</span>

<style>
  .activity {
    flex: none;
    display: inline-flex;
    align-items: center;
  }
  .track {
    display: block;
    width: 44px;
    height: 6px;
    border-radius: var(--rt-radius-pill);
    background: var(--rt-surface-sunken);
    overflow: hidden;
  }
  .bar {
    display: block;
    height: 100%;
    border-radius: var(--rt-radius-pill);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>

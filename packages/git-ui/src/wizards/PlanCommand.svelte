<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The Tailor Kit Authors -->
<!-- GitTailor wave 4: the Git command a plan equals (`GitPlan.command`), copyable. Shows nothing
     when the product set no command. -->
<script lang="ts">
  import type { GitPlan } from "../types";
  import { Button } from "@tailor-kit/ui";
  import { planCommand } from "./model";

  let { plan }: { plan: Pick<GitPlan, "command"> | null } = $props();
  /** The button says what happened (no toast: the wizard is a dialog). */
  let copied = $state<"" | "Copied" | "Select the text to copy">("");
  const text = $derived(planCommand(plan));

  async function copy(t: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(t);
      copied = "Copied";
    } catch {
      // No clipboard (permissions, insecure context): the command is selectable.
      copied = "Select the text to copy";
    }
  }
</script>

{#if text}
  <div class="command" data-testid="git-plan-command">
    <div class="head">
      <h3 class="caption">Git command</h3>
      <Button size="sm" variant="ghost" icon="copy" data-testid="git-plan-command-copy" onclick={() => copy(text)}>{copied || "Copy"}</Button>
    </div>
    <pre><code>{text}</code></pre>
  </div>
{/if}

<style>
  .command {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-1);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .caption {
    margin: 0;
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    letter-spacing: var(--rt-tracking-caps);
    text-transform: uppercase;
  }
  pre {
    margin: 0;
    padding: var(--rt-space-2);
    overflow-x: auto;
    border-radius: var(--rt-radius-sm);
    background: var(--rt-bg-panel);
    font-family: var(--rt-font-mono);
    font-size: 0.9em;
    user-select: text;
    white-space: pre;
  }
</style>

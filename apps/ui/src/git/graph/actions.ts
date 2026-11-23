// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: runs what a graph menu entry stands for (`model.ts` `GraphAction`): a
// wizard of strand G (`openGitWizard`, the graph reloads afterwards), a Git view (compare,
// diff), copying an id or a name, or selecting a commit; and turns the menus into `Menu`
// entries.

import type { MenuEntry } from "../../ui";
import type { AppState } from "../../state/app.svelte";
import type { GitViews } from "../views.svelte";
import { openGitWizard } from "../wizards/open";
import type { GraphAction, GraphMenuEntry } from "./model";

export interface ActionHost {
  app: AppState;
  views: GitViews;
  /** After a wizard did something. */
  reload: () => void;
  select: (rev: string) => void;
  /** A short confirmation ("Copied"). */
  notify?: (text: string) => void;
}

export async function runAction(host: ActionHost, action: GraphAction): Promise<void> {
  switch (action.kind) {
    case "wizard": {
      const result = await openGitWizard(host.app, action.request);
      if (result) host.reload();
      return;
    }
    case "view":
      host.views.open(action.view);
      return;
    case "copy":
      try {
        await navigator.clipboard.writeText(action.text);
        host.notify?.(`Copied ${action.text.length > 12 ? action.text.slice(0, 7) : action.text}`);
      } catch {
        // No clipboard (permissions, insecure context): the id is selectable in the details.
        host.notify?.("Copying is not allowed here; select the text instead");
      }
      return;
    case "select":
      host.select(action.rev);
      return;
  }
}

/** The graph menus as `Menu` entries that run their actions. */
export function toMenu(entries: readonly GraphMenuEntry[], host: ActionHost): MenuEntry[] {
  return entries.map((e) => {
    if ("separator" in e) return e;
    const action = e.action;
    return {
      id: e.id,
      label: e.label,
      icon: e.icon,
      hint: e.hint,
      danger: e.danger,
      disabled: e.disabled,
      items: e.items ? toMenu(e.items, host) : undefined,
      run: action ? () => void runAction(host, action) : undefined,
    };
  });
}

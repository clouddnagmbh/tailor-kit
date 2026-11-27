// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// Drops in the commit graph (since tailor-kit 0.3, GitTailor wave 3, strand A): what an
// extension offers when a commit row or a branch label is dragged onto another row or label
// (`GitExtension.graphDrops`). The graph accepts a drop only when an extension offers an action
// and shows the choices in a menu at the drop point; the actions plan before they run. Kept
// apart from the view so the rules are tested without rendering it.

import type { MenuEntry } from "@tailor-kit/ui";

import type { ExtensionAction, GraphDropTarget, ProvidedGitExtensions } from "../extensions";

/** One action offered for a drop, keyed `<extension id>/<action id>`. */
export interface GraphDropRef {
  key: string;
  extension: string;
  action: ExtensionAction;
}

/** The actions the extensions offer for `drop`, in extension order; a failing extension is
 *  skipped (and logged), never the drag. */
export function graphDropActions(provided: ProvidedGitExtensions | null, drop: GraphDropTarget): GraphDropRef[] {
  if (!provided) return [];
  return provided.extensions.flatMap((x) => {
    try {
      return (x.graphDrops?.(provided.ctx, drop) ?? []).map((action) => ({ key: `${x.id}/${action.id}`, extension: x.id, action }));
    } catch (e) {
      console.warn(`git-ui: graph drops of extension ${x.id} failed:`, e);
      return [];
    }
  });
}

/** The key a drop's choices are cached under while the drag moves over the same place. */
export function dropKey(drop: GraphDropTarget): string {
  const s = drop.source.kind === "commit" ? `c:${drop.source.commit}` : `r:${drop.source.ref}`;
  return `${s}>${drop.commit}:${drop.ref ?? ""}:${drop.position}`;
}

/** Upper or lower half of a row of height `height` whose top is at `top` (client px). */
export function dropPosition(clientY: number, top: number, height: number): "above" | "below" {
  return clientY < top + height / 2 ? "above" : "below";
}

/** The choices as menu entries; a failing action reports through `onerror`. */
export function dropEntries(refs: readonly GraphDropRef[], onerror: (message: string) => void): MenuEntry[] {
  const fail = (e: unknown) => onerror(e instanceof Error ? e.message : String(e));
  return refs.map(
    (r): MenuEntry => ({
      id: r.key,
      label: r.action.label,
      icon: r.action.icon,
      run: () => {
        try {
          const out = r.action.run();
          if (out instanceof Promise) out.catch(fail);
        } catch (e) {
          fail(e);
        }
      },
    }),
  );
}

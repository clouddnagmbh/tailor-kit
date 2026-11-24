// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Git contributions of a distribution's extensions (since wave 15): actions in the Git views,
// views in the main area's Git host, the staging view of the commit form, diff and merge editors
// and actions next to the commit message. Collected here for the places that show them (strand
// E mounts them; the Git views of strands A to D call these functions). Generic Git: nothing
// here assumes a collection.

import type {
  AppExtension,
  CommitFormTarget,
  ExtensionAction,
  ExtensionFileEditor,
  ExtensionGitView,
  ExtensionHint,
  ExtensionStaging,
  GitActionTarget,
  GitHintTarget,
} from "../extensions";
import type { MenuEntry } from "../ui";
import type { ExtensionActionRef } from "./extension-actions";
import type { ExtensionHintRef } from "./extension-hints";
import type { ProvidedExtensions } from "./extension-tabs";

function collect<A extends { id: string }>(
  provided: ProvidedExtensions | null,
  place: string,
  get: (x: AppExtension) => readonly A[] | undefined,
): ExtensionActionRef<A>[] {
  if (!provided) return [];
  return provided.extensions.flatMap((x) => {
    try {
      return (get(x) ?? []).map((action) => ({ key: `${x.id}/${action.id}`, extension: x.id, action }));
    } catch (e) {
      console.warn(`RequestTailor: ${place} of extension ${x.id} failed:`, e);
      return [];
    }
  });
}

export function extensionGitActions(provided: ProvidedExtensions | null, target: GitActionTarget): ExtensionActionRef<ExtensionAction>[] {
  return collect(provided, "Git actions", (x) => x.gitActions?.(provided!.ctx, target));
}

/** The actions of `target` as menu entries behind a separator (none: an empty list), for the
 *  context menus of the Git views. A failing action reports through `onerror` and never
 *  closes the app. */
export function gitActionEntries(
  provided: ProvidedExtensions | null,
  target: GitActionTarget,
  onerror: (message: string) => void,
): MenuEntry[] {
  const refs = extensionGitActions(provided, target);
  if (refs.length === 0) return [];
  const fail = (e: unknown) => onerror(e instanceof Error ? e.message : String(e));
  return [
    { separator: true },
    ...refs.map(
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
    ),
  ];
}

/** `items` of a commit's menu with the extensions' `ref` actions added to the submenu of each
 *  branch or tag (the entries whose id is `ref:<full name>`). */
export function withRefActions(
  provided: ProvidedExtensions | null,
  repository: string,
  items: MenuEntry[],
  onerror: (message: string) => void,
): MenuEntry[] {
  return items.map((e) => {
    if (!("label" in e) || !e.id?.startsWith("ref:") || !e.items) return e;
    const more = gitActionEntries(provided, { kind: "ref", repository, ref: e.id.slice(4) }, onerror);
    return more.length === 0 ? e : { ...e, items: [...e.items, ...more] };
  });
}

/** Every hint for a Git view, in extension order (dismissed ones included); pick one with
 *  `pickHint` of ./extension-hints. */
export function extensionGitHints(provided: ProvidedExtensions | null, target: GitHintTarget): ExtensionHintRef[] {
  return collect<ExtensionHint>(provided, "Git hints", (x) => x.gitHints?.(provided!.ctx, target)).map((r) => ({
    key: r.key,
    extension: r.extension,
    hint: r.action,
  }));
}

export function extensionCommitFormActions(provided: ProvidedExtensions | null, target: CommitFormTarget): ExtensionActionRef<ExtensionAction>[] {
  return collect(provided, "commit form actions", (x) => x.commitFormActions?.(provided!.ctx, target));
}

/** Every Git view, keyed `<extension>/<view>` (the `id` of a `GitView` of kind `extension`). */
export function extensionGitViews(provided: ProvidedExtensions | null): ExtensionActionRef<ExtensionGitView>[] {
  return collect(provided, "Git views", (x) => x.gitViews?.(provided!.ctx));
}

/** The Git view `key` (`<extension>/<view>`), if an extension still offers it. */
export function extensionGitView(provided: ProvidedExtensions | null, key: string): ExtensionGitView | null {
  return extensionGitViews(provided).find((r) => r.key === key)?.action ?? null;
}

/** Diff (`kind` `diff`) or merge editors that accept `path`. */
export function extensionFileEditors(
  provided: ProvidedExtensions | null,
  kind: ExtensionFileEditor["kind"],
  path: string,
): ExtensionActionRef<ExtensionFileEditor>[] {
  return collect(provided, "file editors", (x) => x.fileEditors?.(provided!.ctx)).filter((r) => {
    if (r.action.kind !== kind) return false;
    try {
      return r.action.accepts ? r.action.accepts(path) : true;
    } catch (e) {
      console.warn(`RequestTailor: file editor ${r.key} failed:`, e);
      return false;
    }
  });
}

/** The staging view in charge for `repository`: the first extension's whose `active` agrees.
 *  `null`: the commit form commits without staging. */
export function extensionStaging(
  provided: ProvidedExtensions | null,
  repository: string,
): { extension: string; staging: ExtensionStaging } | null {
  if (!provided) return null;
  for (const x of provided.extensions) {
    const staging = x.staging;
    if (!staging) continue;
    try {
      if (staging.active && !staging.active(provided.ctx, repository)) continue;
    } catch (e) {
      console.warn(`RequestTailor: staging of extension ${x.id} failed:`, e);
      continue;
    }
    return { extension: x.id, staging };
  }
  return null;
}

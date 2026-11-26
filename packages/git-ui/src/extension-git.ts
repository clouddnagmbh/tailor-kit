// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Git contributions of a distribution's extensions (since wave 15): actions in the Git views,
// views in the main area's Git host, the staging view of the commit form, diff and merge editors
// and actions next to the commit message. Collected here for the places that show them (strand
// E mounts them; the Git views of strands A to D call these functions). Generic Git: nothing
// here assumes a collection. Moved from RequestTailor's `shell/extension-git.ts` in step K3.

import type { MenuEntry } from "@tailor-kit/ui";

import type {
  CommitFormTarget,
  ExtensionAction,
  ExtensionFileEditor,
  ExtensionGitView,
  ExtensionHint,
  ExtensionStaging,
  GitActionTarget,
  GitExtension,
  GitHintTarget,
  ProvidedGitExtensions,
  RevisionRangeTarget,
} from "./extensions";

type ProvidedExtensions = ProvidedGitExtensions;

/** One contribution of an extension, keyed `<extension id>/<contribution id>`. */
export interface ExtensionActionRef<A> {
  /** `<extension id>/<action id>`. */
  key: string;
  extension: string;
  action: A;
}

/** A hint of an extension. */
export interface ExtensionHintRef {
  /** `<extension id>/<hint id>`, the key of its dismissal. */
  key: string;
  extension: string;
  hint: ExtensionHint;
}

function collect<A extends { id: string }>(
  provided: ProvidedExtensions | null,
  place: string,
  get: (x: GitExtension) => readonly A[] | undefined,
): ExtensionActionRef<A>[] {
  if (!provided) return [];
  return provided.extensions.flatMap((x) => {
    try {
      return (get(x) ?? []).map((action) => ({ key: `${x.id}/${action.id}`, extension: x.id, action }));
    } catch (e) {
      console.warn(`git-ui: ${place} of extension ${x.id} failed:`, e);
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
      console.warn(`git-ui: file editor ${r.key} failed:`, e);
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
      console.warn(`git-ui: staging of extension ${x.id} failed:`, e);
      continue;
    }
    return { extension: x.id, staging };
  }
  return null;
}

/** Actions on two revisions selected together in the graph. */
export function extensionRangeActions(provided: ProvidedExtensions | null, target: RevisionRangeTarget): ExtensionActionRef<ExtensionAction>[] {
  return collect(provided, "range actions", (x) => x.rangeActions?.(provided!.ctx, target));
}

/** The key a dismissal of `ref` is stored under: just the hint for scope `device` (the default),
 *  the hint plus the repository for scope `target`. */
export function dismissalKey(ref: ExtensionHintRef, target: GitHintTarget): string {
  return ref.hint.scope !== "target" ? ref.key : `${ref.key}@git:${target.repository}`;
}

/** The hint to show: the first of `refs` not dismissed (at most one per place). */
export function pickHint(refs: readonly ExtensionHintRef[], target: GitHintTarget, dismissed: ReadonlySet<string>): ExtensionHintRef | null {
  return refs.find((r) => !dismissed.has(dismissalKey(r, target))) ?? null;
}

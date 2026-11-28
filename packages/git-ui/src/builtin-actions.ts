// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// "Blame" and "File history" as actions on file targets (file tree, file view, changes list,
// commit file lists), offered by the kit itself. They are independent of `GitHost.content`: a
// product without files of its own (GitTailor) gets them without writing an extension. With
// `content` the views behave as before and the palette entries ("Git: Blame this request") stay
// the way to blame the active file.
//
// Replacing them: an extension that offers an action with the id `blame` (or `file-history`) on a
// file target replaces the kit's action of that name, whatever its label or extension id.

import { extensionGitActions } from "./extension-git";
import type { ExtensionAction, GitActionTarget, GitExtension, ProvidedGitExtensions } from "./extensions";
import type { GitHost } from "./host";
import type { GitViews } from "./views.svelte";

/** Id of the kit's own extension (keys `git/blame`, `git/file-history`). */
export const BUILTIN_EXTENSION_ID = "git";
/** Action ids a product's extension uses to replace the kit's action of that name. */
export const BLAME_ACTION_ID = "blame";
export const FILE_HISTORY_ACTION_ID = "file-history";

type FileTarget = Extract<GitActionTarget, { kind: "file" }>;

/** The kit's extension for `views`; it adds nothing to the actions of other targets. */
export function builtinFileExtension(provided: ProvidedGitExtensions | null, views: GitViews): GitExtension {
  const replaced = (target: FileTarget, id: string): boolean =>
    (provided?.extensions ?? []).some((x) => {
      try {
        return (x.gitActions?.(provided!.ctx, target) ?? []).some((a) => a.id === id);
      } catch {
        return false; // a failing extension reports itself elsewhere; it replaces nothing
      }
    });
  return {
    id: BUILTIN_EXTENSION_ID,
    gitActions(_ctx, target): readonly ExtensionAction[] {
      if (target.kind !== "file" || target.directory || !target.repository) return [];
      const out: ExtensionAction[] = [];
      if (!replaced(target, BLAME_ACTION_ID)) {
        out.push({ id: BLAME_ACTION_ID, label: "Blame", icon: "commit", run: () => views.open({ kind: "blame", path: target.path, rev: target.rev }) });
      }
      // The history of a file at a revision is the history of the file.
      if (!replaced(target, FILE_HISTORY_ACTION_ID)) {
        out.push({ id: FILE_HISTORY_ACTION_ID, label: "File history", icon: "history", run: () => views.open({ kind: "file-history", path: target.path }) });
      }
      return out;
    },
  };
}

/** `provided` with the kit's file actions behind the product's extensions. With `content` (or
 *  without views to open) it is `provided` itself. */
export function withBuiltinFileActions(
  provided: ProvidedGitExtensions | null,
  host: Pick<GitHost, "content">,
  views: GitViews | undefined,
): ProvidedGitExtensions | null {
  if (host.content || !views) return provided;
  const own = builtinFileExtension(provided, views);
  return provided ? { ...provided, extensions: [...provided.extensions, own] } : { extensions: [own], ctx: {} as ProvidedGitExtensions["ctx"] };
}

/** One action of an extension as a command of a command palette (since tailor-kit 0.8.1). */
export interface ActionCommand {
  /** `<extension id>/<action id>`, unique for the target. */
  key: string;
  extension: string;
  label: string;
  icon?: string;
  badge?: string;
  group?: ExtensionAction["group"];
  /** Runs the action. A failing action reports through `onerror` and never throws. */
  run: () => void;
}

/** Every action every extension offers on `target`, plus the kit's own file actions, as flat
 *  commands for a palette: one place that lists what the menus of the views list, so a product's
 *  palette finds every action without knowing the extensions. `provided` is
 *  `GitHost.extensions` (already carrying the built-in file actions if the product wants them,
 *  see `withBuiltinFileActions`). A failing extension is skipped (it reports itself elsewhere). */
export function commandsFor(
  provided: ProvidedGitExtensions | null,
  target: GitActionTarget,
  onerror: (message: string) => void = () => {},
): ActionCommand[] {
  const fail = (e: unknown) => onerror(e instanceof Error ? e.message : String(e));
  return extensionGitActions(provided, target).map((r) => ({
    key: r.key,
    extension: r.extension,
    label: r.action.label,
    icon: r.action.icon,
    badge: r.action.badge,
    group: r.action.group,
    run: () => {
      try {
        const out = r.action.run();
        if (out instanceof Promise) out.catch(fail);
      } catch (e) {
        fail(e);
      }
    },
  }));
}

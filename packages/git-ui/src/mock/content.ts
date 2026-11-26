// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// What the mock Git leaves to the product: what a file is (`MockContent`: how it reads as text,
// when two are the same, how three versions merge) and the fixtures (`MockFixtures`: the demo
// repository, the scenarios, the words of the product). The defaults here are plain text files,
// so a product without a model of its own (GitTailor) gets a working mock with no more code;
// RequestTailor binds request specs (`apps/ui/src/commands/mock-git-content.ts`).

import type { Core } from "./actions";

/** The result of a three-way merge of one file. */
export interface MockFileMerge<S> {
  path: string;
  /** What the merge leaves in the working tree (ours when it conflicts). */
  merged: S | undefined;
  /** How many changes of the other side applied cleanly. */
  clean: number;
  /** What conflicts, in words (field names for a model, a sentence for text). */
  conflicts: string[];
}

/** What a file is to the mock. `S` is the product's model of a file; plain text by default. */
export interface MockContent<S> {
  /** The file as the text the Git views show (diffs, blobs, the secret scan). */
  render(file: S): string;
  /** Whether two files read the same: what makes a file "modified". */
  same(a: S | undefined, b: S | undefined): boolean;
  /** Whether two files hold the same data (merges and the wizards' previews compare this). */
  identical(a: S | undefined, b: S | undefined): boolean;
  /** Merges one file; `null`: there is nothing to do (both sides the same). */
  merge(path: string, base: S | undefined, ours: S | undefined, theirs: S | undefined): MockFileMerge<S> | null;
}

/** The mock's scenarios (`MockGit.scenario`): `none` is no repository, `clean` one commit in
 *  sync with origin; the others are on top of it (see `MockGit.scenario`). */
export type MockGitScenario =
  | "none"
  | "clean"
  | "conflict"
  | "empty"
  | "no-remote"
  | "unpublished"
  | "ahead"
  | "behind"
  | "diverged"
  | "detached"
  | "showcase";

/** A file of the repository next to the modelled ones, for the viewers: text, an image
 *  (`base64`, `media`) or a binary of `binary` bytes. */
export interface MockExtraFile {
  text?: string;
  base64?: string;
  media?: string;
  binary?: number;
}

/** One commit of a scenario: a change to `path` with its message. */
export interface MockScenarioEdit<S> {
  path: string;
  message: string;
  /** The file as the working tree has it (`undefined`: it is not there) → the new file. */
  apply(base: S | undefined): S;
}

/** The content the scenarios need; the engine (`runScenario`) brings the structure: branches,
 *  merges, a tag, what is ahead, behind or changed in the working tree. */
export interface MockScenarioEdits<S> {
  /** `conflict`: a remote and a local commit change the same file. */
  conflictRemote: MockScenarioEdit<S>;
  conflictLocal: MockScenarioEdit<S>;
  /** `ahead`, `diverged`, `detached` and the commit on `main` in `showcase`. */
  ahead: MockScenarioEdit<S>;
  /** `behind`: two remote commits; `diverged` takes the second. */
  remoteA: MockScenarioEdit<S>;
  remoteB: MockScenarioEdit<S>;
  /** `showcase`: three commits on a feature branch, two commits on `main` after the merge. */
  branchA: MockScenarioEdit<S>;
  branchB: MockScenarioEdit<S>;
  branchC: MockScenarioEdit<S>;
  mainA: MockScenarioEdit<S>;
  mainB: MockScenarioEdit<S>;
  /** `showcase`: the working tree changes a file, adds one and deletes one. */
  workModified: { path: string; apply(base: S | undefined): S };
  workAdded: { path: string; file: S };
  workDeleted: { path: string };
}

/** The product's fixtures and words. */
export interface MockFixtures<S> {
  /** The unit the product works on, for messages ("The collection is not in a Git repository"). */
  scope: string;
  /** A file of the model, singular ("request file"), for messages. */
  fileNoun: string;
  /** `refs/<namespace>/backup/…` */
  refNamespace: string;
  /** The author of the commits the app makes itself (the backups): the product's name. */
  author: string;
  /** Whether a merge of the file at `path` is field by field (`GitMergeFile.semantic`). */
  semantic(path: string): boolean;
  /** `GitWorkingCopy.repository` and the id of the repository the mock stands for. */
  repository: string;
  /** The message of the first commit. */
  initialMessage: string;
  /** The message of the first commit of the synthetic history (`?graph=<n>`). */
  syntheticFirst: string;
  /** The files of a mock without a host (unit tests). */
  initialFiles(): Map<string, S>;
  /** Sets up `name` on top of `clean` (`first` is its commit): `conflict`, `ahead`, … Everything
   *  else has the structure of `runScenario`. */
  scenario(core: Core<S>, name: MockGitScenario, first: string): void;
  /** Files of the repository beside the modelled ones: what `HEAD` and the working tree hold. */
  extraFiles: {
    committed: Record<string, MockExtraFile>;
    working: Record<string, MockExtraFile | null>;
    ignored: string[];
  };
  /** The fields the product's JSON adds to a changed file (RequestTailor: its collection).
   *  `path` is repository-relative without a leading `/`; `from` says where the file is listed:
   *  the commit graph (`graph`) or the working copy (`working`, only modelled files). */
  tag(path: string, from: "graph" | "working"): object;
}

const clone = <T>(v: T): T => structuredClone(v);

/** Plain text files: equal when equal, merged whole (one side changed, or a conflict). */
export const textContent: MockContent<string> = {
  render: (file) => file,
  same: (a, b) => a === b,
  identical: (a, b) => a === b,
  merge(path, base, ours, theirs) {
    if (ours === theirs || base === theirs) return null;
    if (base === ours) return { path, merged: theirs, clean: 1, conflicts: [] };
    const why =
      ours === undefined ? "deleted here, changed there" : theirs === undefined ? "changed here, deleted there" : base === undefined ? "added on both sides" : "changed on both sides";
    return { path, merged: ours, clean: 0, conflicts: [why] };
  },
};

/** The structure of the scenarios on top of `clean`, with the product's content (`edits`). */
export function runScenario<S>(core: Core<S>, name: MockGitScenario, first: string, edits: MockScenarioEdits<S>): void {
  const files = (e: MockScenarioEdit<S>): Record<string, S> => ({ [e.path]: e.apply(core.work.get(e.path)) });
  switch (name) {
    case "conflict": {
      core.remoteCommit(files(edits.conflictRemote), edits.conflictRemote.message);
      const ours = edits.conflictLocal.apply(core.work.get(edits.conflictLocal.path));
      core.writeWork(edits.conflictLocal.path, ours);
      const tree = clone(core.store.get(first)!.tree);
      tree.set(edits.conflictLocal.path, clone(ours));
      core.branchesMap.set("main", core.newCommit([first], tree, edits.conflictLocal.message, "Demo Author"));
      return;
    }
    case "empty":
      core.store.clear();
      core.branchesMap.clear();
      core.remote = null;
      core.remoteUrls.clear();
      core.fetched.clear();
      core.tracking.clear();
      return;
    case "no-remote":
      core.remote = null;
      core.remoteUrls.clear();
      core.fetched.clear();
      core.tracking.clear();
      return;
    case "unpublished":
      core.branchesMap.set("feature/login", first);
      core.current = "feature/login";
      return;
    case "ahead":
      core.localCommit(files(edits.ahead), edits.ahead.message, "Ada Lovelace");
      return;
    case "behind":
      core.remoteCommit(files(edits.remoteA), edits.remoteA.message);
      core.remoteCommit(files(edits.remoteB), edits.remoteB.message);
      core.fetched.set("main", core.remote!.get("main")!);
      return;
    case "diverged":
      core.localCommit(files(edits.ahead), edits.ahead.message, "Ada Lovelace");
      core.remoteCommit(files(edits.remoteB), edits.remoteB.message);
      core.fetched.set("main", core.remote!.get("main")!);
      return;
    case "detached":
      core.localCommit(files(edits.ahead), edits.ahead.message, "Ada Lovelace");
      core.remote!.set("main", core.branchesMap.get("main")!);
      core.fetched.set("main", core.branchesMap.get("main")!);
      core.current = null;
      core.detachedAt = first;
      core.moveWork(core.store.get(core.branchesMap.get("main")!)!.tree, core.store.get(first)!.tree);
      return;
    case "showcase": {
      const users = core.localCommit(files(edits.branchA), edits.branchA.message, "Grace Hopper");
      core.branchesMap.set("feature/auth", users);
      core.current = "feature/auth";
      core.localCommit(files(edits.branchB), edits.branchB.message, "Linus Torvalds");
      const auth = core.localCommit(files(edits.branchC), edits.branchC.message, "Linus Torvalds");
      core.current = "main";
      core.moveWork(core.store.get(auth)!.tree, core.store.get(users)!.tree);
      core.localCommit(files(edits.ahead), edits.ahead.message, "Ada Lovelace");
      const mainTip = core.branchesMap.get("main")!;
      const merged = clone(core.store.get(mainTip)!.tree);
      for (const [p, file] of core.store.get(auth)!.tree) if (!core.content.same(core.store.get(users)!.tree.get(p), file)) merged.set(p, clone(file));
      const merge = core.newCommit([mainTip, auth], merged, "Merge feature/auth into main", "Ada Lovelace");
      core.branchesMap.set("main", merge);
      core.moveWork(core.store.get(mainTip)!.tree, merged);
      core.tagsMap.set("v1.0", { oid: merge, message: "First release" });
      core.remote!.set("main", merge);
      core.remote!.set("feature/auth", auth);
      core.fetched.set("main", merge);
      core.fetched.set("feature/auth", auth);
      core.localCommit(files(edits.mainA), edits.mainA.message, "Ada Lovelace");
      core.localCommit(files(edits.mainB), edits.mainB.message, "Grace Hopper");
      // Three changes in the working tree: modified, new, deleted.
      core.writeWork(edits.workModified.path, edits.workModified.apply(core.work.get(edits.workModified.path)));
      core.writeWork(edits.workAdded.path, clone(edits.workAdded.file));
      core.writeWork(edits.workDeleted.path, undefined);
      return;
    }
    default:
      return;
  }
}

const edit = (path: string, message: string, apply: (text: string) => string): MockScenarioEdit<string> => ({
  path,
  message,
  apply: (base) => apply(base ?? ""),
});

const TEXT_EDITS: MockScenarioEdits<string> = {
  conflictRemote: edit("README.md", "Remote change to the readme", (t) => `${t}Remote line.\n`),
  conflictLocal: edit("README.md", "Local change to the readme", (t) => `${t}Local line.\n`),
  ahead: edit("README.md", "Describe the project better", () => "# Demo\n\nA small project with a longer description.\n"),
  remoteA: edit("README.md", "Retitle the readme", () => "# Demo, version 2\n\nA small project.\n"),
  remoteB: edit("src/app.txt", "Update the app", () => "app v2\n"),
  branchA: edit("docs/guide.md", "Start the guide", () => "# Guide\n\nStep one.\n"),
  branchB: edit("src/lib.txt", "Update the library", () => "lib v2\n"),
  branchC: edit("src/app.txt", "Rewrite the app", () => "app v3\n"),
  mainA: edit("src/lib.txt", "Extend the library", (t) => `${t}more\n`),
  mainB: edit("docs/old.txt", "Say what the old notes are", () => "Old notes, kept for reference.\n"),
  workModified: { path: "README.md", apply: (base) => `${base ?? ""}(edited here)\n` },
  workAdded: { path: "src/new.txt", file: "new\n" },
  workDeleted: { path: "docs/blocked.txt" },
};

/** Plain text fixtures: a small project, the scenarios of the Git panel. */
export const textFixtures: MockFixtures<string> = {
  scope: "repository",
  fileNoun: "file",
  refNamespace: "gittailor",
  author: "GitTailor",
  semantic: () => false,
  repository: "mock://demo",
  initialMessage: "Initial commit",
  syntheticFirst: "Start the project",
  initialFiles: () =>
    new Map([
      ["README.md", "# Demo\n\nA small project.\n"],
      ["src/app.txt", "app v1\n"],
      ["src/lib.txt", "lib v1\n"],
      ["docs/guide.md", "# Guide\n"],
      ["docs/old.txt", "Old notes.\n"],
      ["docs/blocked.txt", "Blocked.\n"],
    ]),
  scenario: (core, name, first) => runScenario(core, name, first, TEXT_EDITS),
  extraFiles: { committed: {}, working: {}, ignored: [] },
  tag: () => ({}),
};

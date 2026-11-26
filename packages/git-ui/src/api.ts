// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// What the Git views call on a product's backend: Git, repositories and pull-request review
// (provenance: RequestTailor `apps/ui/src/commands/backend.ts`, moved here in step K3). A product
// implements them in every shell (desktop, web, mock) and hands them to the views through
// `GitHost.backend`. Adding or changing a command is an interface change.

import type {
  CiStatus,
  DeviceCodePrompt,
  GitAction,
  GitActionResult,
  GitBlame,
  GitBlob,
  GitBranch,
  GitChangedFile,
  GitCloneRequest,
  GitCommit,
  GitCommitDetails,
  GitCommitWrite,
  GitConflict,
  GitDiff,
  GitFileCommit,
  GitGraphPage,
  GitGraphQuery,
  GitHistoryPage,
  GitHistoryQuery,
  GitIdentity,
  GitIndexDiff,
  GitPatchRequest,
  GitPatchSide,
  GitPickRequest,
  GitPickResult,
  GitPlan,
  GitPullResult,
  GitRef,
  GitReflogEntry,
  GitRemote,
  GitRepoSummary,
  GitResolution,
  GitSignatureCheck,
  GitStagedCommit,
  GitStash,
  GitStatus,
  GitTree,
  GitTreeEntry,
  GitTreeWrite,
  GitUndoEntry,
  GitWorkingCopy,
  NewPullRequest,
  NewReviewComment,
  PullRequest,
  RepositoryEntry,
  ReviewComment,
  ReviewHost,
  ReviewRepo,
  ReviewSession,
  ReviewSubmission,
  SecretFinding,
} from "./types";

/** Where a clone landed, for the product to show (RequestTailor: a collection). */
export interface GitOwnerInfo {
  /** Stable id on this device. */
  id: string;
  /** Display name. */
  name: string;
  /** Where it lives, for display only. */
  location: string;
}

/** Git for the repository that holds the open collection (rt_app::git describes the model:
 *  no staging area, paths relative to the collection). Operations that change the working
 *  tree (`checkout`, `pull`, `resolve`) reject with kind `git` while there are unsaved
 *  changes; afterwards the UI calls `AppState.reload()`. Remote operations reject with kind
 *  `git-auth` (message: the host) when credentials are missing or wrong.
 *  Web: `push`, `pull` and `clone` go through the companion and reject with kind
 *  `unsupported` without it. Owners in wave 4: strands A1 (desktop) and A2 (web, UI). */
export interface GitApi<F extends object = {}> {
  status(): Promise<GitStatus>;
  /** `HEAD` → working tree of one file. */
  diff(path: string): Promise<GitDiff>;
  /** Commits exactly `paths` (new, changed or deleted) with `message`. */
  commit(message: string, paths: string[]): Promise<GitCommit>;
  /** Commits of the current branch, newest first. */
  log(limit: number): Promise<GitCommit[]>;
  branches(): Promise<GitBranch[]>;
  /** Switches to a local branch, or to a remote one (`origin/x` → local `x` tracking it). */
  checkout(branch: string): Promise<void>;
  createBranch(name: string, checkout: boolean): Promise<void>;
  /** Pushes the current branch to its upstream (sets `origin/<branch>` if it has none). */
  push(): Promise<void>;
  /** Fetches and merges the upstream into the current branch. */
  pull(): Promise<GitPullResult>;
  conflict(path: string): Promise<GitConflict>;
  resolve(path: string, resolution: GitResolution): Promise<void>;
  /** Creates a repository in the collection folder. */
  init(): Promise<void>;
  /** Clones `url` (HTTPS, or on desktop a local repository path / `file://` URL) into a new folder inside one the user picks and opens it; `null` if
   *  cancelled. */
  clone(url: string): Promise<GitOwnerInfo | null>;
  /** Desktop only (absent elsewhere): shows the folder picker for a repository on this
   *  machine and returns its path for the clone field; `null` if cancelled. `clone` also
   *  accepts the path forms `~/…`, `file://…` and relative paths. */
  pickRepository?(): Promise<string | null>;
  /** Author for commits; `null` if none is configured. */
  identity(): Promise<GitIdentity | null>;
  setIdentity(identity: GitIdentity): Promise<void>;
  /** HTTPS credentials per host, kept like secret values (keychain / encrypted storage). */
  hasCredentials(host: string): Promise<boolean>;
  setCredentials(host: string, username: string, password: string): Promise<void>;
  deleteCredentials(host: string): Promise<void>;
  /** Credentials typed into `paths` (as they are on disk). `commit` runs the same scan and
   *  rejects with kind `secrets-found` when it finds any. Owner in wave 5: strand E. */
  scan(paths: string[]): Promise<SecretFinding[]>;

  // Wave 12 (interface): revisions, read without a checkout. A `rev` is a full or abbreviated
  // commit id, a branch (`main`, `origin/main`), a ref (`refs/...`) or `HEAD`, optionally
  // followed by `~<n>` or `^`. Paths are relative to the collection root.
  /** The commit `rev` names (full id). Rejects with kind `git` for an unknown rev. */
  revParse(rev: string): Promise<string>;
  /** The collection's files at `rev`, as the shells read them from disk. */
  treeAt(rev: string): Promise<GitTree>;
  /** Text of `path` at `rev`, `null` if it does not exist there. A leading `/` makes the path
   *  relative to the repository root (the web: the collection folder is the root). */
  show(rev: string, path: string): Promise<string | null>;
  /** Commits of `HEAD`'s history that changed `path`, newest first (renames: strand D). */
  fileLog(path: string, limit: number): Promise<GitFileCommit[]>;
  /** Best common ancestor; `null` if the histories are unrelated. */
  mergeBase(a: string, b: string): Promise<string | null>;
  /** Commits reachable from `to` along first parents but not from `from` (`null`: down to
   *  the root), newest first. */
  commits(from: string | null, to: string, limit: number): Promise<GitCommit[]>;

  // Wave 14 (interface): Git for the whole repository of the active collection, generic (no
  // assumption that it is a collection). Paths repository-relative with a leading `/`.
  // Strand F: the commit graph and what a commit changed.
  /** A page of the commit graph (all refs, topological, newest first). */
  graph(query: GitGraphQuery): Promise<GitGraphPage>;
  /** All branches, remote branches, tags and a detached `HEAD`, with ahead/behind. */
  refs(): Promise<GitRef[]>;
  /** Files changed from `from` to `to` (`null`: `to` against its first parent). */
  changes(from: string | null, to: string): Promise<GitChangedFile<F>[]>;
  /** Line diff of any file between two revs (`to` `null`: the working tree). */
  fileDiff(from: string | null, to: string | null, path: string): Promise<GitDiff>;
  // Strand G: the wizards. `plan` only reads; `apply` plans again and rejects with kind `git`
  // and `GitPlan.blocked.message` when blocked, kind `git-auth` when credentials are needed.
  remotes(): Promise<GitRemote[]>;
  stashes(): Promise<GitStash[]>;
  plan(action: GitAction): Promise<GitPlan>;
  apply(action: GitAction): Promise<GitActionResult>;
  /** The newest entry of the action journal, `null` if there is none. */
  lastAction(): Promise<GitUndoEntry | null>;
  /** Takes the entry `id` back (refs, working tree); rejects when it is `stale`. */
  undo(id: string): Promise<GitActionResult>;

  // Wave 15 (interface): a Git client for any repository. Every method acts on the *active
  // repository* (`CollectionSet.active_repository`); paths are repository-relative with a
  // leading `/`. Until a strand lands, a method rejects with kind `unsupported`.
  // Strand B: the working copy, the file tree, file contents.
  /** Status of the whole working tree against `HEAD` (and the index). */
  workingCopy(): Promise<GitWorkingCopy<F>>;
  /** One folder of the file tree (`dir` `"/"` for the root): at `rev`, or (`null`) the working
   *  tree with states and ignored files. */
  fileTree(rev: string | null, dir: string): Promise<GitTreeEntry[]>;
  /** A file for the viewers: at `rev`, or (`null`) in the working tree. */
  blob(rev: string | null, path: string): Promise<GitBlob>;
  // Strand C: inspecting the history of any file.
  /** Who last changed each line of `path` at `rev`. */
  blame(rev: string, path: string): Promise<GitBlame>;
  searchHistory(query: GitHistoryQuery): Promise<GitHistoryPage>;
  /** The reflog of `name` (`HEAD`, `refs/heads/main`, or a short branch name), newest first. */
  reflog(name: string, limit: number): Promise<GitReflogEntry[]>;
  // Strand D: building blocks for history editors; nothing moves until `apply` of a
  // `rewrite` action. And the 3-way text conflict view's result.
  commitInfo(rev: string): Promise<GitCommitDetails>;
  pick(request: GitPickRequest): Promise<GitPickResult>;
  /** The id of the tree written. */
  writeTree(write: GitTreeWrite): Promise<string>;
  /** The id of the commit written (no ref moves). */
  writeCommit(write: GitCommitWrite): Promise<string>;
  /** Writes `text` as the resolution of the conflicted file `path` and marks it resolved
   *  (refused with unsaved changes; the next `commit` concludes the merge). */
  resolveText(path: string, text: string): Promise<void>;
  // Strand F: the index, for a staging view an extension adds (the app's own commit form
  // keeps "commit without staging"), and signatures.
  indexDiff(path: string, side: GitPatchSide): Promise<GitIndexDiff>;
  stage(paths: string[]): Promise<void>;
  unstage(paths: string[]): Promise<void>;
  /** Stages the chosen lines of an `unstaged` diff (refused when the file changed since). */
  stagePatch(patch: GitPatchRequest): Promise<void>;
  /** Unstages the chosen lines of a `staged` diff. */
  unstagePatch(patch: GitPatchRequest): Promise<void>;
  /** Takes the chosen lines of an `unstaged` diff out of the working tree. */
  discardPatch(patch: GitPatchRequest): Promise<void>;
  /** Commits what the index holds (secret scan as `commit`). */
  commitStaged(commit: GitStagedCommit): Promise<GitCommit>;
  /** Signature state of each commit; without a verifier `unverified` or `unsigned`. */
  verify(oids: string[]): Promise<GitSignatureCheck[]>;
  /** An effective Git configuration value (`commit.template`, `user.signingkey`, `gpg.format`,
   *  `commit.gpgsign`, ...) or `null` when unset: natively the repository's own configuration,
   *  then the user's global and system configuration; in the browser the repository's own. */
  config(key: string): Promise<string | null>;
}

/** Wave 15 (interface, strand A): repositories in the collection set. The list is part of
 *  `CollectionSet` (`repositories`, `active_repository`); these calls change it. */
export interface GitRepositoryApi<R extends object = {}> {
  /** Makes repository `id` the active one (every `GitApi` command then acts on it). */
  activate(id: string): Promise<RepositoryEntry<R>>;
  /** Branch, ahead/behind and number of changes of any repository of the set. */
  summary(id: string): Promise<GitRepoSummary>;
  /** Clones into a new folder, adds it as a root and makes it active; `null` if cancelled. */
  clone(request: GitCloneRequest): Promise<RepositoryEntry<R> | null>;
  /** Folder picker, then `git init` there (an existing repository is refused), added as a root
   *  and made active; `null` if cancelled. */
  init(): Promise<RepositoryEntry<R> | null>;
  /** Desktop only (absent in the browser and the mock): adds an existing folder that is a Git
   *  repository (a worktree or submodule checkout an extension made, say) as a root and makes
   *  its repository active, without a picker. Refused for a folder outside Git. The UI then
   *  calls `AppState.adoptRepository(entry)`. */
  openPath?(path: string): Promise<RepositoryEntry<R>>;
}


/** Pull requests (GitHub) and merge requests (GitLab) of the collection's `origin`, with the
 *  user's own token (keychain / encrypted browser storage). Owner in wave 12: strand E. */
export interface ReviewApi {
  /** The Git host and project of `origin`; `null` if it is none the app knows. */
  repo(): Promise<ReviewRepo | null>;
  /** Self-hosted Git hosts the user added (GitHub Enterprise, GitLab), per device;
   *  `github.com` and `gitlab.com` need none. */
  hosts(): Promise<ReviewHost[]>;
  setHosts(hosts: ReviewHost[]): Promise<void>;
  hasToken(host: string): Promise<boolean>;
  setToken(host: string, token: string): Promise<void>;
  deleteToken(host: string): Promise<void>;
  /** Device-code login: the code to show; then `finishLogin` waits for the user. */
  startLogin(host: string): Promise<DeviceCodePrompt>;
  finishLogin(host: string): Promise<void>;
  list(): Promise<PullRequest[]>;
  /** Fetches the PR's head into a local ref (no checkout) for diff and runs. */
  open(number: number): Promise<ReviewSession>;
  comments(number: number): Promise<ReviewComment[]>;
  comment(number: number, comment: NewReviewComment): Promise<ReviewComment>;
  submit(number: number, review: ReviewSubmission): Promise<void>;
  ci(number: number): Promise<CiStatus>;
  /** Wave 15 (strand E): opens a pull request from a branch already on the remote. */
  create(pull: NewPullRequest): Promise<PullRequest>;
}

/** Errors from a backend are rejected promises carrying this shape. */
export class BackendError extends Error {
  constructor(
    readonly kind: string,
    message: string,
  ) {
    super(message);
    this.name = "BackendError";
  }
}

/** The parts of a product's backend the Git views call. */
export interface GitHostBackend {
  readonly git: GitApi;
  readonly repositories: GitRepositoryApi;
  readonly review: ReviewApi;
  readonly kind: "tauri" | "web" | "mock";
}
export type { GitHistoryMethods, GitIndexMethods, GitPlumbingMethods, GitWorkingMethods } from "./git-methods";

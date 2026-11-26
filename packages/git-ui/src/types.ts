// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

// The contract types of the Git views and of pull-request review: TypeScript mirrors of the serde
// JSON of the Rust side (provenance: RequestTailor `apps/ui/src/commands/types.ts`, moved here in
// step K3). A product keeps its contract test, which annotates real serialized values with these
// types, so `npm run check` fails on any drift. Changing these types is an interface change.
// `ContentDiff`, `MergeState` and `TreeNode` are the field-level diff, merge and file-tree
// shapes the views read when the host provides owned content (`GitContent`).
//
// Where the JSON of a product carries a field the kit does not know (RequestTailor: the owner of
// a file), the type takes an extension parameter that defaults to `{}`: `GitWorkingFile<E>`,
// `GitChangedFile<E>`, `RepositoryEntry<E>`, `GitWorkingCopy<E>`. The product binds it in its own
// `types.ts` and the kit views, which never read such a field, work with the default.

export type TreeNode =
  | { kind: "folder"; name: string; path: string; children: TreeNode[] }
  | { kind: "request"; name: string; path: string; method: string };

export type GitFileState = "untracked" | "modified" | "deleted" | "conflicted";

export interface GitFile {
  path: string;
  state: GitFileState;
}

export interface GitStatus {
  /** `false`: the collection is not in a Git repository. */
  repo: boolean;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: GitFile[];
  merging: boolean;
}

export interface GitBranch {
  name: string;
  current: boolean;
  remote: boolean;
}

export interface GitCommit {
  oid: string;
  summary: string;
  author: string;
  /** Unix seconds. */
  time: number;
}

export type GitPullResult =
  | { outcome: "up-to-date" }
  | { outcome: "fast-forward"; commits: number }
  | { outcome: "merged"; oid: string }
  | { outcome: "conflicts"; paths: string[] };

export interface GitConflict {
  path: string;
  base: string | null;
  ours: string | null;
  theirs: string | null;
}

export type GitResolution = "ours" | "theirs";

export interface GitIdentity {
  name: string;
  email: string;
}

export type DiffLineKind = "context" | "added" | "removed";

export interface DiffLine {
  kind: DiffLineKind;
  text: string;
}

export interface DiffHunk {
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  lines: DiffLine[];
}

export interface GitDiff {
  path: string;
  binary: boolean;
  hunks: DiffHunk[];
}

export interface DeviceCodePrompt {
  user_code: string;
  verification_uri: string;
  verification_uri_complete: string | null;
  expires_in: number;
}

export interface SecretFinding {
  path: string;
  line: number;
  kind: string;
  snippet: string;
  hint: string;
}

/** The collection's files at a commit, read without a checkout (`GitApi.treeAt`). */
export interface GitTree {
  /** The commit the rev resolved to. */
  oid: string;
  /** Path relative to the collection root → text (`*.yml` as `FileSet::from_dir` reads). */
  files: Record<string, string>;
}

export type GitFileChange = "added" | "modified" | "deleted" | "renamed";

export interface GitFileCommit {
  commit: GitCommit;
  /** The file's path at this commit. */
  path: string;
  change: GitFileChange;
  /** The path before a rename. */
  from?: string;
}

/** A model field, e.g. `["headers", "Authorization"]` (see `rt_format::diff`). */
export type FieldPath = string[];

/** What a field-level change is about; the product defines the kinds (the kit only passes it on). */
export interface ChangeTarget {
  kind: string;
}

export type ChangeKind =
  | { kind: "added" }
  | { kind: "removed" }
  | { kind: "modified" }
  | { kind: "moved"; from: string };

export type DiffValue =
  | { kind: "text"; text: string }
  | { kind: "json"; value: unknown }
  /** A secret's value: never shown. */
  | { kind: "secret" };

export interface FieldChange {
  path: FieldPath;
  old: DiffValue | null;
  new: DiffValue | null;
  /** 1-based line of the field in the new file (old file if removed). */
  line?: number;
}

export interface ModelChange<T extends ChangeTarget = ChangeTarget> {
  target: T;
  change: ChangeKind;
  file: string;
  name: string;
  fields: FieldChange[];
}

export type FileStatus = "added" | "removed" | "modified";

export interface ChangedFile {
  path: string;
  status: FileStatus;
  /** `false`: the file also differs in ways the model does not hold; offer its line diff. */
  covered: boolean;
}

/** The field-level diff of the files a product understands (RequestTailor: `CollectionDiff`). */
export interface ContentDiff<T extends ChangeTarget = ChangeTarget> {
  changes: ModelChange<T>[];
  files: ChangedFile[];
}

/** What the Git view diffs. */
export type DiffScope =
  | { kind: "working" }
  | { kind: "commit"; rev: string }
  | { kind: "between"; from: string; to: string };

export interface FieldConflict<T extends ChangeTarget = ChangeTarget> {
  target: T;
  file: string;
  field: FieldPath;
  base: DiffValue | null;
  ours: DiffValue | null;
  theirs: DiffValue | null;
}

export interface MergeState<T extends ChangeTarget = ChangeTarget> {
  merging: boolean;
  conflicts: FieldConflict<T>[];
  text_conflicts: string[];
  /** Files Git left conflicted that merge cleanly on the model; `resolve` writes them too. */
  clean?: string[];
}

export type ReviewKind = "github" | "gitlab";

export interface ReviewHost {
  host: string;
  kind: ReviewKind;
  api: string;
}

export interface ReviewRepo {
  kind: ReviewKind;
  host: string;
  api: string;
  project: string;
}

export type PullState = "open" | "closed" | "merged";

export interface PullRequest {
  number: number;
  title: string;
  author: string;
  state: PullState;
  draft: boolean;
  source: string;
  target: string;
  head: string;
  base: string;
  url: string;
  updated: number;
}

export interface ReviewComment {
  id: string;
  author: string;
  body: string;
  created: number;
  path: string | null;
  line: number | null;
  field?: FieldPath;
  outdated: boolean;
}

export interface NewReviewComment {
  path: string;
  line: number;
  body: string;
}

export type ReviewVerdict = "approve" | "request-changes" | "comment";

export interface ReviewSubmission {
  verdict: ReviewVerdict;
  body: string;
}

export type CiState = "pending" | "success" | "failure" | "neutral" | "none";

export interface CiCheck {
  name: string;
  state: CiState;
  url: string | null;
}

export interface CiStatus {
  state: CiState;
  checks: CiCheck[];
}

export interface ReviewSession {
  pull: PullRequest;
  head_ref: string;
  merge_base: string;
}

export type ReviewError =
  | { kind: "no-token"; message: string }
  | { kind: "auth"; message: string }
  | { kind: "not-found"; message: string }
  | { kind: "transport"; message: string }
  | { kind: "host"; message: string };

export type GitRefKind = "local" | "remote" | "tag" | "head";

export interface GitRef {
  /** `refs/heads/main`, `refs/remotes/origin/main`, `refs/tags/v1`, `HEAD`. */
  full: string;
  /** As shown: `main`, `origin/main`, `v1`, `HEAD`. */
  name: string;
  kind: GitRefKind;
  target: string;
  current: boolean;
  upstream?: string;
  ahead: number;
  behind: number;
  /** Annotated tags: first line of the message. */
  message?: string;
}

export interface GitGraphQuery {
  skip?: number;
  limit: number;
  /** Full ref names to start from; absent: all refs and `HEAD`. */
  refs?: string[];
}

export interface GitGraphCommit extends GitCommit {
  /** First parent first; two or more: a merge commit. */
  parents: string[];
  email: string;
  /** Full names of the refs at this commit. */
  refs?: string[];
  /** Wave 15: carries a signature (`GitApi.verify` checks it). */
  signed?: boolean;
}

/** Topological order, newest first; all refs on every page. */
export interface GitGraphPage {
  commits: GitGraphCommit[];
  refs: GitRef[];
  head?: string;
  detached: boolean;
  more: boolean;
}

export type GitChangedFile<E extends object = {}> = {
  path: string;
  change: GitFileChange;
  from?: string;
  binary: boolean;
} & E;

export type GitResetMode = "keep-changes" | "discard";

/** What a wizard asks Git to do (`rt_app::git::actions::GitAction`). Treat an unknown
 *  `action` as unsupported: wave 15 adds more. */
export type GitAction =
  | { action: "create-branch"; name: string; start: string; switch?: boolean }
  | { action: "switch"; target: string }
  | { action: "rename-branch"; from: string; to: string }
  | { action: "delete-branch"; name: string; force?: boolean }
  | { action: "merge"; from: string }
  | { action: "pull" }
  | { action: "push" }
  | { action: "publish"; branch: string; remote: string }
  | { action: "fetch"; remote?: string }
  | { action: "add-remote"; name: string; url: string }
  | { action: "remove-remote"; name: string }
  | { action: "rename-remote"; from: string; to: string }
  | { action: "set-remote-url"; name: string; url: string }
  | { action: "stash"; message?: string; paths?: string[] }
  | { action: "unstash"; index: number; keep?: boolean }
  | { action: "drop-stash"; index: number }
  | { action: "revert"; commit: string }
  | { action: "cherry-pick"; commit: string }
  | { action: "reset"; target: string; mode: GitResetMode }
  | { action: "discard"; paths: string[] }
  | { action: "create-tag"; name: string; target: string; message?: string }
  | { action: "delete-tag"; name: string }
  | { action: "push-tag"; name: string; remote: string }
  // Wave 15 (strand D):
  | { action: "amend"; message?: string; paths?: string[]; staged?: boolean }
  | { action: "abort" }
  | { action: "restore-branch"; name: string; target: string }
  | { action: "restore-files"; rev: string; paths: string[] }
  | { action: "rewrite"; branch: string; tip: string; summary: string };

/** Why an action cannot run now; the UI explains each and offers the way out. */
export type GitBlockReason =
  | "unsaved-changes"
  | "local-changes"
  | "in-progress"
  | "detached"
  | "no-upstream"
  | "diverged"
  | "nothing-to-do"
  | "invalid-name"
  | "exists"
  | "not-found"
  | "current-branch"
  | "not-merged"
  | "rejected"
  | "auth-required"
  | "network"
  | "unsupported"
  /** Wave 15: it would replace commits that are already on a remote. */
  | "published";

export interface GitBlock {
  reason: GitBlockReason;
  message: string;
}

export interface GitRefMove {
  name: string;
  before: string | null;
  after: string | null;
}

export interface GitMergeFile {
  path: string;
  /** Merged field by field (a collection file) or line by line. */
  semantic: boolean;
  clean: number;
  conflicts: string[];
}

export interface GitMergePreview {
  fast_forward: boolean;
  commits: GitCommit[];
  files: GitMergeFile[];
}

/** What an action would do, computed without changing anything. */
export interface GitPlan {
  action: GitAction;
  steps: string[];
  refs: GitRefMove[];
  files?: string[];
  network: boolean;
  backup: boolean;
  undoable: boolean;
  merge?: GitMergePreview;
  destructive: boolean;
  warnings?: string[];
  blocked?: GitBlock;
}

export interface GitUndoEntry {
  id: string;
  /** The `GitAction` tag. */
  action: string;
  summary: string;
  time: number;
  refs: GitRefMove[];
  backups?: string[];
  stash?: string;
  /** Set when refs moved since: undo is refused. */
  stale?: string;
}

export interface GitActionResult {
  message: string;
  undo?: GitUndoEntry;
  outcome?: GitPullResult;
  /** The working tree changed: reload every collection of the repository. */
  reload: boolean;
}

export interface GitRemote {
  name: string;
  url: string;
  push_url?: string;
}

/** A parked change (`stash@{index}`). */
export interface GitStash {
  index: number;
  oid: string;
  message: string;
  time: number;
  branch?: string;
  files: string[];
}

/** A repository of the set (strand A). */
export type RepositoryEntry<E extends object = {}> = {
  /** The id the product's own entries name the repository by. */
  id: string;
  name: string;
  /** For display only. */
  location: string;
  /** The root it was found through. */
  root: string;
  /** A linked worktree or a submodule's checkout. */
  linked: boolean;
} & E;

/** A merge, cherry-pick, revert or rebase that stopped half way. */
export type GitOperation = "merge" | "cherry-pick" | "revert" | "rebase";

/** What the sidebar shows next to a repository. */
export interface GitRepoSummary {
  id: string;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  /** Changed, new and conflicted files of the whole working tree. */
  changes: number;
  operation?: GitOperation;
}

/** Clone any repository into a new folder and add it to the set. */
export interface GitCloneRequest {
  url: string;
  /** Folder to clone into; absent: the folder picker. */
  parent?: string;
  /** New folder's name; absent: from the URL. */
  name?: string;
  branch?: string;
}

export type GitEntryKind = "file" | "dir" | "symlink" | "submodule";

/** A changed file of the working copy (strand B). */
export type GitWorkingFile<E extends object = {}> = {
  path: string;
  /** `HEAD` → working tree. */
  state: GitFileState;
  /** Renames: the old path. */
  from?: string;
  /** Absent: a file. */
  kind?: GitEntryKind;
  /** `HEAD` → index, if the index differs from `HEAD`. */
  staged?: GitFileChange;
  /** The working tree differs from the index. */
  unstaged: boolean;
} & E;

export interface GitWorkingCopy<E extends object = {}> {
  repository: string;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  head?: string;
  detached: boolean;
  operation?: GitOperation;
  /** Sorted by path; ignored files left out; at most 10 000 (`truncated`). */
  files: GitWorkingFile<E>[];
  truncated: boolean;
}

/** One entry of a folder in the file tree. */
export interface GitTreeEntry {
  name: string;
  path: string;
  kind: GitEntryKind;
  size?: number;
  /** Working tree only. */
  state?: GitFileState;
  ignored: boolean;
}

/** A file's content for the viewers: text up to 8 MiB, images up to 16 MiB as base64, else
 *  metadata only. */
export interface GitBlob {
  path: string;
  /** Absent: the working tree. */
  rev?: string;
  oid?: string;
  size: number;
  binary: boolean;
  media_type?: string;
  text?: string;
  base64?: string;
  truncated: boolean;
  lfs_pointer: boolean;
}

/** Consecutive lines from the same commit (strand C). */
export interface GitBlameHunk {
  /** 1-based. */
  start: number;
  lines: number;
  commit: GitCommit;
  /** The path in that commit. */
  path: string;
  orig_start: number;
}

export interface GitBlame {
  path: string;
  rev: string;
  lines: number;
  hunks: GitBlameHunk[];
  binary: boolean;
}

/** Every given field must match; case-insensitive substrings. */
export interface GitHistoryQuery {
  message?: string;
  author?: string;
  /** Path prefix, repository-relative with a leading `/`. */
  path?: string;
  /** Added or removed lines containing this (`git log -S`, case-insensitive). */
  content?: string;
  since?: number;
  until?: number;
  refs?: string[];
  skip?: number;
  limit: number;
}

export type GitHistoryField = "message" | "author" | "path" | "content";

export interface GitHistoryHit {
  commit: GitGraphCommit;
  matched: GitHistoryField[];
  paths?: string[];
}

export interface GitHistoryPage {
  hits: GitHistoryHit[];
  more: boolean;
  scanned: number;
}

/** One reflog entry, newest first. */
export interface GitReflogEntry {
  name: string;
  index: number;
  old: string | null;
  new: string;
  message: string;
  time: number;
  commit?: GitCommit;
}

/** Author or committer (strand D). */
export interface GitSignature {
  name: string;
  email: string;
  time: number;
  /** Minutes east of UTC. */
  offset: number;
}

export interface GitCommitDetails {
  oid: string;
  tree: string;
  parents: string[];
  message: string;
  author: GitSignature;
  committer: GitSignature;
  signed: boolean;
}

/** Replay `commit`'s change onto `onto` in memory (cherry-pick, or revert). */
export interface GitPickRequest {
  onto: string;
  commit: string;
  revert?: boolean;
  /** Merge commits: the parent (1-based) that counts as base. */
  mainline?: number;
}

export interface GitPickResult {
  /** The merged tree; conflicted paths hold `onto`'s version. */
  tree: string;
  clean: boolean;
  conflicts?: GitConflict[];
  /** Conflicts that are not text: take a side with `writeTree`. */
  other_conflicts?: string[];
}

export type GitTreeContent =
  | { kind: "text"; text: string }
  | { kind: "base64"; data: string }
  /** The path as in another tree or commit. */
  | { kind: "from"; rev: string }
  | { kind: "delete" };

export type GitTreeFile = { path: string; executable?: boolean } & GitTreeContent;

export interface GitTreeWrite {
  /** A tree or commit rev. */
  base: string;
  files: GitTreeFile[];
}

export interface GitCommitWrite {
  tree: string;
  parents: string[];
  message: string;
  /** Absent: the app's identity, now. */
  author?: GitSignature;
  committer?: GitSignature;
}

/** Which pair of versions a diff compares (strand F). */
export type GitPatchSide = "unstaged" | "staged";

export interface GitIndexDiff {
  side: GitPatchSide;
  diff: GitDiff;
  /** Blob ids of both sides (`""`: missing there). */
  old: string;
  new: string;
}

export interface GitHunkSelection {
  /** Index into `diff.hunks`. */
  hunk: number;
  /** Indexes into the hunk's `lines` (added and removed lines only); absent: the whole hunk. */
  lines?: number[];
}

export interface GitPatchRequest {
  path: string;
  side: GitPatchSide;
  /** `GitIndexDiff.old`/`new` of the diff the selection was made on. */
  old: string;
  new: string;
  hunks: GitHunkSelection[];
}

export interface GitStagedCommit {
  message: string;
}

export type GitSignatureKind = "gpg" | "ssh" | "x509";

export type GitSignatureState = "unsigned" | "unverified" | "good" | "untrusted" | "bad" | "unknown-key" | "expired";

export interface GitSignatureCheck {
  oid: string;
  state: GitSignatureState;
  kind?: GitSignatureKind;
  signer?: string;
  key?: string;
}

/** Open a pull request from a branch on the remote (strand E, GitHub and GitLab). */
export interface NewPullRequest {
  title: string;
  body: string;
  /** Branch with the changes, as named on the remote. */
  head: string;
  /** Branch to merge into. */
  base: string;
  draft: boolean;
}

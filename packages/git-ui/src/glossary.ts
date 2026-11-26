// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14 interface: Git words in plain language, for tooltips and "What does this mean?" in the
// graph (strand F), the wizards and the Git panel (strand G) and the comparison (strand H). One
// list, so every view explains a word the same way. Strand G owns the texts; adding terms is fine.

import type { GitBrand } from "./host";

export type GitTerm =
  | "repository"
  | "commit"
  | "branch"
  | "current-branch"
  | "remote"
  | "remote-branch"
  | "upstream"
  | "ahead"
  | "behind"
  | "diverged"
  | "head"
  | "detached"
  | "merge"
  | "fast-forward"
  | "conflict"
  | "pull"
  | "push"
  | "fetch"
  | "publish"
  | "stash"
  | "tag"
  | "revert"
  | "cherry-pick"
  | "reset"
  | "backup"
  | "clone"
  | "amend"
  | "abort"
  | "restore"
  | "rewrite"
  | "published";

export interface GitGloss {
  /** The word as the UI shows it ("Park changes" rather than "stash"). */
  label: string;
  /** One or two sentences without other jargon. */
  text: string;
  /** The Git word, for people who know Git or search the web. */
  git: string;
}

export const GLOSSARY: Record<GitTerm, GitGloss> = {
  repository: { label: "Repository", git: "repository", text: "A folder whose history Git keeps: every saved version of every file." },
  commit: { label: "Commit", git: "commit", text: "A saved version of your files, with a message saying what changed. You can always go back to it." },
  branch: { label: "Branch", git: "branch", text: "A line of work with its own commits. Work on a branch without touching the others, and merge it back when it is ready." },
  "current-branch": { label: "Current branch", git: "checked-out branch", text: "The branch your files show right now. New commits go onto it." },
  remote: { label: "Remote", git: "remote", text: "A copy of the repository on a server (for example GitHub), usually called origin. You send and get commits from it." },
  "remote-branch": { label: "Remote branch", git: "remote-tracking branch", text: "What a branch on the server looked like when the app last checked." },
  upstream: { label: "Linked branch", git: "upstream", text: "The branch on the server your branch sends to and gets from." },
  ahead: { label: "To send", git: "ahead", text: "Commits you have that the server does not have yet. Send them with Push." },
  behind: { label: "To get", git: "behind", text: "Commits on the server you do not have yet. Get them with Pull." },
  diverged: { label: "Went apart", git: "diverged", text: "You and the server both have new commits. Merge them to bring both together." },
  head: { label: "You are here", git: "HEAD", text: "The commit your files show right now." },
  detached: { label: "Not on a branch", git: "detached HEAD", text: "You are looking at an older commit. New commits here belong to no branch unless you create one." },
  merge: { label: "Merge", git: "merge", text: "Brings the commits of another branch into the current one. Fields changed on both sides need your decision." },
  "fast-forward": { label: "Move forward", git: "fast-forward", text: "The current branch has nothing of its own, so it simply moves to the newer commit. No merge commit needed." },
  conflict: { label: "Conflict", git: "conflict", text: "The same field or line was changed on both sides. You choose which version to keep." },
  pull: { label: "Get changes", git: "pull", text: "Gets new commits from the server and merges them into your branch." },
  push: { label: "Send changes", git: "push", text: "Sends your commits to the server so others can get them." },
  fetch: { label: "Check for changes", git: "fetch", text: "Asks the server what is new, without changing your files." },
  publish: { label: "Publish branch", git: "push --set-upstream", text: "Puts a new branch on the server for the first time and links the two." },
  stash: { label: "Park changes", git: "stash", text: "Puts your uncommitted changes aside so your files are clean. Bring them back later." },
  tag: { label: "Tag", git: "tag", text: "A fixed name for one commit, such as a release (v1.2.0). Unlike a branch, it does not move." },
  revert: { label: "Undo a commit", git: "revert", text: "Makes a new commit that takes back what an earlier commit did. History stays as it is." },
  "cherry-pick": { label: "Copy a commit here", git: "cherry-pick", text: "Applies the changes of one commit from another branch to the current one, as a new commit." },
  reset: { label: "Move branch back", git: "reset", text: "Moves the current branch to an earlier commit. The app keeps a backup, so you can undo it." },
  backup: { label: "Backup", git: "refs/{namespace}/backup", text: "Before anything that changes history or your files, the app saves where everything was, so Undo can put it back." },
  clone: { label: "Clone", git: "clone", text: "Copies a repository from a server (or a folder) to this computer, with its whole history." },
  amend: { label: "Change last commit", git: "commit --amend", text: "Replaces your most recent commit with a new one: a better message, or files you forgot. Only for commits that are not on the server yet." },
  abort: { label: "Stop and go back", git: "merge --abort", text: "Stops a merge or copy that needs your decision and puts your files back as they were before it started." },
  restore: { label: "Restore", git: "restore", text: "Takes files, or a deleted branch, back to how they were in an earlier commit. The app keeps a backup of what it replaces." },
  rewrite: { label: "Rewrite history", git: "rebase", text: "Replaces some of a branch's commits with new ones, for example to drop or combine them. Only for commits that are not on the server yet." },
  published: { label: "On the server", git: "pushed", text: "Commits that others may have already. Changing them would make their copies disagree with yours, so the app does not." },
};

/** What the word is called in Git; `{namespace}` is the product's `GitBrand.refNamespace`. */
export function gitWord(term: GitTerm, brand: GitBrand): string {
  return GLOSSARY[term].git.replace("{namespace}", brand.refNamespace);
}

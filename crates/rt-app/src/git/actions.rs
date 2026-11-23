// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Git actions for the wizards (wave 14, strand G): one [`GitAction`] per thing a user can ask
//! for, a [`GitPlan`] that says in plain words what it would do *before* it runs, and a
//! [`GitActionResult`] with an undo entry afterwards.
//!
//! Flow: the wizard builds a [`GitAction`], calls `GitApi.plan` (reads only), shows the plan
//! (ref moves, files touched, merge preview, backup), and on confirmation `GitApi.apply` with
//! the same action. `apply` plans again and refuses with kind `git` and the plan's
//! [`GitBlock::message`] when it is blocked now. Natively in `rt-git` (gitoxide), in the browser
//! with isomorphic-git; both answer with these types.
//!
//! Rules, the same for every shell:
//! - Whatever moves a ref or changes the working tree writes a backup first:
//!   `refs/requesttailor/backup/<unix>/<name>` ([`backup_ref`], as the wave-12 secret rescue),
//!   and an [`GitUndoEntry`] to the repository's action journal (`.git/requesttailor/actions.jsonl`,
//!   never committed, at most [`UNDO_KEEP`] entries). `GitApi.undo` puts the refs (and, if `HEAD`
//!   moved, the working tree) back where the entry says; it is refused once the refs moved since.
//! - Working-tree changes are refused while the app has unsaved changes
//!   ([`GitBlockReason::UnsavedChanges`]); afterwards every collection of the repository reloads.
//! - Merge, cherry-pick and revert merge semantically where a file is a collection file
//!   (wave 12, `rt_format::merge`) and line by line everywhere else; conflicts end in the
//!   wave-12 conflict views (`MergeApi`, `GitApi.conflict`/`resolve`).
//! - No force push and no history rewrite of published commits from the app (interactive rebase
//!   comes with wave 15). A diverged branch offers merge or "take theirs" (reset with backup).
//! - Generic: nothing assumes the repository is a collection. Paths are repository-relative with
//!   a leading `/`. New actions are new variants (wave 15 adds more), so the UI must ignore
//!   unknown [`GitPlan`] fields and treat an unknown action as unsupported.

use serde::{Deserialize, Serialize};

use super::{GitCommit, GitPullResult};

/// Prefix of the backup refs, the same as the secret rescue's.
pub use crate::rescue::BACKUP_PREFIX;

/// Entries kept in the action journal; older ones can no longer be undone.
pub const UNDO_KEEP: usize = 20;

/// The backup ref for `name` (a short branch or tag name, or `HEAD`) at `unix` seconds.
pub fn backup_ref(unix: i64, name: &str) -> String {
    format!("{BACKUP_PREFIX}{unix}/{name}")
}

/// Whether `name` is a valid branch or tag name, in plain words when it is not (the rules of
/// `git check-ref-format --branch`, which both shells apply before asking Git).
pub fn check_ref_name(name: &str) -> Result<(), GitBlock> {
    let bad = |why: &str| {
        Err(GitBlock {
            reason: GitBlockReason::InvalidName,
            message: format!("\"{name}\" cannot be used as a name: {why}."),
        })
    };
    if name.is_empty() {
        return bad("it is empty");
    }
    if name == "HEAD" || name == "@" {
        return bad("Git uses it itself");
    }
    if name.starts_with('-') || name.starts_with('/') || name.ends_with('/') {
        return bad("it must not start with \"-\" or \"/\" or end with \"/\"");
    }
    if name.ends_with('.') || name.ends_with(".lock") {
        return bad("it must not end with \".\" or \".lock\"");
    }
    if name.contains("..") || name.contains("//") || name.contains("@{") {
        return bad("it must not contain \"..\", \"//\" or \"@{\"");
    }
    if name
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
    {
        return bad("it must not contain spaces or any of ~ ^ : ? * [ \\");
    }
    if name.split('/').any(|part| part.starts_with('.')) {
        return bad("no part of it may start with \".\"");
    }
    Ok(())
}

/// How far a reset goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitResetMode {
    /// Move the branch; the files keep their content and show as changes ("undo commits, keep
    /// the work").
    KeepChanges,
    /// Move the branch and make the files match it ("throw away commits and changes").
    Discard,
}

/// Something a wizard asks Git to do. `rev`s as in wave 12 (id, branch, ref, `HEAD`, `~n`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum GitAction {
    /// New local branch at `start` ("branch from here"); `switch` checks it out.
    CreateBranch {
        name: String,
        start: String,
        #[serde(default)]
        switch: bool,
    },
    /// Check out a local branch, a remote one (creates the tracking branch) or, detached, a
    /// commit or tag.
    Switch {
        target: String,
    },
    RenameBranch {
        from: String,
        to: String,
    },
    /// `force`: also when its commits are on no other branch (the plan says how many).
    DeleteBranch {
        name: String,
        #[serde(default)]
        force: bool,
    },
    /// Merge `from` (branch, remote branch, tag, commit) into the current branch.
    Merge {
        from: String,
    },
    /// Fetch, then merge the upstream into the current branch (as `GitApi.pull`).
    Pull,
    /// Push the current branch to its upstream.
    Push,
    /// Push `branch` to `remote` for the first time and set it as upstream.
    Publish {
        branch: String,
        remote: String,
    },
    Fetch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remote: Option<String>,
    },
    AddRemote {
        name: String,
        url: String,
    },
    RemoveRemote {
        name: String,
    },
    RenameRemote {
        from: String,
        to: String,
    },
    SetRemoteUrl {
        name: String,
        url: String,
    },
    /// "Park changes": a Git stash (`refs/stash`, so `git stash list` sees it).
    Stash {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Apply a parked change; `keep`: leave it in the list.
    Unstash {
        index: u32,
        #[serde(default)]
        keep: bool,
    },
    DropStash {
        index: u32,
    },
    /// A new commit that undoes `commit` on the current branch.
    Revert {
        commit: String,
    },
    /// Apply `commit` on the current branch as a new commit.
    CherryPick {
        commit: String,
    },
    /// Move the current branch to `target`.
    Reset {
        target: String,
        mode: GitResetMode,
    },
    /// Throw away unsaved-in-Git changes of these files (repository-relative with a leading
    /// `/`, or relative to the collection without one); empty: all.
    Discard {
        paths: Vec<String>,
    },
    /// `message`: an annotated tag.
    CreateTag {
        name: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    DeleteTag {
        name: String,
    },
    PushTag {
        name: String,
        remote: String,
    },
}

/// Why an action cannot run now. The UI explains each in plain words and offers the way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitBlockReason {
    /// The app has unsaved edits (save or discard them first).
    UnsavedChanges,
    /// Files differ from the last commit and the action would overwrite them (park or commit).
    LocalChanges,
    /// A merge (or cherry-pick, revert) is half done; finish or abort it first.
    InProgress,
    /// `HEAD` is not on a branch.
    Detached,
    /// The branch has no upstream yet (publish it).
    NoUpstream,
    /// Both sides have commits the other lacks (merge, or take theirs).
    Diverged,
    /// Nothing to do (already merged, nothing to push, nothing to park, ...).
    NothingToDo,
    InvalidName,
    /// A branch, tag or remote of that name exists.
    Exists,
    NotFound,
    /// The checked-out branch cannot be deleted or renamed away like this.
    CurrentBranch,
    /// Deleting would lose commits that are on no other branch (`force` to confirm).
    NotMerged,
    /// The remote has commits the branch lacks (pull first).
    Rejected,
    /// The remote wants credentials (as `AppError::GitAuth`).
    AuthRequired,
    Network,
    /// Not possible in this shell (e.g. stdio-only remotes in the browser).
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBlock {
    pub reason: GitBlockReason,
    /// Plain words, no Git jargon without an explanation.
    pub message: String,
}

/// A ref the action creates, moves or deletes (`None`: does not exist before / after). In a
/// [`GitPlan`], `after: Some("")` means "a new commit the action creates" (a merge commit, a
/// cherry-pick, a revert, a pull, a stash), whose id is not known before it runs; journal
/// entries ([`GitUndoEntry::refs`]) always carry the real ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRefMove {
    /// Full name (`refs/heads/x`, `refs/tags/v1`, `HEAD`).
    pub name: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Per file of a merge, cherry-pick or revert preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitMergeFile {
    /// Repository-relative, leading `/`.
    pub path: String,
    /// Merged semantically (a collection file) or line by line.
    pub semantic: bool,
    /// Fields (semantic) or hunks (lines) that merge cleanly.
    pub clean: u32,
    /// Field paths (semantic, as `rt_format::merge`) or `lines <a>-<b>` that conflict.
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitMergePreview {
    /// The branch only moves forward; no merge commit.
    pub fast_forward: bool,
    /// Commits that come in.
    pub commits: Vec<GitCommit>,
    pub files: Vec<GitMergeFile>,
}

/// What an action would do, computed without changing anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPlan {
    pub action: GitAction,
    /// One sentence each, plain words ("Creates the branch feature from main and switches to it.").
    pub steps: Vec<String>,
    pub refs: Vec<GitRefMove>,
    /// Files in the working tree it changes (repository-relative, leading `/`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// It talks to a remote.
    #[serde(default)]
    pub network: bool,
    /// A backup ref and an undo entry are written.
    #[serde(default)]
    pub backup: bool,
    /// Undo can take it back (pushes cannot).
    #[serde(default)]
    pub undoable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<GitMergePreview>,
    /// It would lose work that no backup covers; the wizard asks once more.
    #[serde(default)]
    pub destructive: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Why it cannot run now; `apply` refuses with this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<GitBlock>,
}

/// One entry of the action journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitUndoEntry {
    pub id: String,
    /// `GitAction` tag (`create-branch`, `merge`, ...).
    pub action: String,
    /// What was done, plain words.
    pub summary: String,
    /// Unix seconds.
    pub time: i64,
    pub refs: Vec<GitRefMove>,
    /// Backup refs written for it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backups: Vec<String>,
    /// The stash that holds the working-tree changes it replaced, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stash: Option<String>,
    /// Refs moved since: undo would be refused (plain words).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitActionResult {
    /// What happened, plain words.
    pub message: String,
    /// `None`: nothing to undo (fetch, push, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<GitUndoEntry>,
    /// Merge, pull, cherry-pick, revert, unstash: how it ended (conflicts → conflict views).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<GitPullResult>,
    /// The working tree changed: the UI reloads every collection of the repository.
    #[serde(default)]
    pub reload: bool,
}

/// A remote, credentials never included in `url`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_url: Option<String>,
}

/// A parked change (`stash@{index}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStash {
    pub index: u32,
    pub oid: String,
    pub message: String,
    pub time: i64,
    /// The branch it was parked on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Repository-relative, leading `/`.
    #[serde(default)]
    pub files: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_names_follow_git_rules_in_plain_words() {
        for ok in ["main", "feature/login", "v1.2.3", "fix-42", "a/b/c"] {
            assert!(check_ref_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "HEAD",
            "-x",
            "/x",
            "x/",
            "x.",
            "x.lock",
            "a..b",
            "a//b",
            "a@{1",
            "my branch",
            "a~1",
            "a^",
            "a:b",
            "a?",
            "a*",
            "a[",
            "a\\b",
            ".hidden",
            "a/.b",
        ] {
            let e = check_ref_name(bad).unwrap_err();
            assert_eq!(e.reason, GitBlockReason::InvalidName, "{bad}");
            assert!(e.message.contains("cannot be used"), "{bad}");
        }
    }

    #[test]
    fn backups_share_the_rescue_prefix() {
        assert_eq!(
            backup_ref(1_700_000_000, "main"),
            "refs/requesttailor/backup/1700000000/main"
        );
    }

    #[test]
    fn actions_are_tagged_kebab_case() {
        let a = GitAction::CreateBranch {
            name: "x".into(),
            start: "HEAD".into(),
            switch: true,
        };
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["action"], "create-branch");
        assert_eq!(serde_json::from_value::<GitAction>(json).unwrap(), a);
        let r: GitAction =
            serde_json::from_str(r#"{"action":"reset","target":"HEAD~1","mode":"keep-changes"}"#)
                .unwrap();
        assert_eq!(
            r,
            GitAction::Reset {
                target: "HEAD~1".into(),
                mode: GitResetMode::KeepChanges
            }
        );
    }
}

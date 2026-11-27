// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! `GitAction::Commit` (GitTailor wave 1, strand F, gate point F-1): a commit runs through plan
//! and apply with a backup ref and a journal entry; undo moves the branch back and keeps the
//! files. Also the two repository options that strand needs: `with_author` (a product's chosen
//! identity wins over the Git configuration) and `with_push_undo` (push and publish can be
//! undone with a lease). Real `git` is only the witness.

mod common;
mod wizard;

use std::sync::Arc;

use common::*;
use tk_git::{ActionOptions, NoCredentials, Repo};
use tk_git_types::actions::{GitAction, GitBlockReason};
use tk_git_types::{CommitScanner, GitIdentity};
use wizard::*;

fn commit(message: &str, paths: &[&str]) -> GitAction {
    GitAction::Commit {
        message: message.into(),
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        staged: false,
    }
}

/// `git status --porcelain` of the work tree, when `git` is installed.
fn porcelain(dir: &std::path::Path) -> Option<String> {
    git(dir, &["status", "--porcelain"])
}

#[test]
fn commit_action_commits_the_files_named_and_undo_keeps_them() {
    let f = fixture("commit-files");
    let head = ref_id(f.dir(), "refs/heads/main").unwrap();
    f.write("notes.txt", "changed\n");
    f.write("loose.txt", "new\n");
    f.write("other.txt", "not committed\n");
    let before = state(f.dir());
    let git_before = porcelain(f.dir());

    let action = commit(
        "Change the notes\n\nWith a body.",
        &["/notes.txt", "/loose.txt"],
    );
    let p = plan(&f.repo(), &action);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.backup && p.undoable && !p.destructive && !p.network);
    assert_eq!(p.files, ["/loose.txt", "/notes.txt"]);
    assert_eq!(p.refs.len(), 1);
    assert_eq!(p.refs[0].name, "refs/heads/main");
    assert_eq!(p.refs[0].before.as_deref(), Some(head.as_str()));
    assert_eq!(p.refs[0].after.as_deref(), Some(""));
    assert!(
        p.steps
            .iter()
            .any(|s| s.contains("2 files") && s.contains("Change the notes")),
        "{:?}",
        p.steps
    );
    // Planning changes nothing.
    let raw = raw_state(f.dir());
    let _ = plan(&f.repo(), &action);
    assert_eq!(raw_state(f.dir()), raw);

    let r = apply(&f.repo(), &action);
    assert!(!r.reload);
    let entry = r.undo.expect("a commit is undoable");
    assert_eq!(entry.action, "commit");
    assert_eq!(entry.backups.len(), 1);
    assert_eq!(
        ref_id(f.dir(), &entry.backups[0]).as_deref(),
        Some(head.as_str())
    );
    let info = f.repo().commit_info("HEAD").unwrap();
    assert_eq!(info.parents, std::slice::from_ref(&head));
    assert_eq!(info.message.trim(), "Change the notes\n\nWith a body.");
    assert_eq!(
        f.repo().show("HEAD", "/notes.txt").unwrap().as_deref(),
        Some("changed\n")
    );
    assert_eq!(
        f.repo().show("HEAD", "/loose.txt").unwrap().as_deref(),
        Some("new\n")
    );
    assert_eq!(f.repo().show("HEAD", "/other.txt").unwrap(), None);
    if let Some(s) = porcelain(f.dir()) {
        assert_eq!(s.trim(), "?? other.txt");
    }
    let last = f.repo().last_action().unwrap().unwrap();
    assert_eq!(last.id, entry.id);
    assert!(last.stale.is_none());

    let undone = f.repo().undo(&entry.id, false).unwrap();
    assert!(undone.message.contains("Undone"));
    assert_eq!(
        ref_id(f.dir(), "refs/heads/main").as_deref(),
        Some(head.as_str())
    );
    assert_eq!(state(f.dir()), before);
    assert_eq!(porcelain(f.dir()), git_before);
    assert_eq!(f.read("notes.txt"), "changed\n");
    assert!(f.repo().last_action().unwrap().is_none());
}

#[test]
fn commit_action_commits_the_index_when_staged() {
    let f = fixture("commit-staged");
    let head = ref_id(f.dir(), "refs/heads/main").unwrap();
    f.write("notes.txt", "staged\n");
    f.write("other.txt", "untouched\n");
    f.repo().stage(&paths(&["/notes.txt"])).unwrap();
    let action = GitAction::Commit {
        message: "Staged only".into(),
        paths: Vec::new(),
        staged: true,
    };
    let p = plan(&f.repo(), &action);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert_eq!(p.files, ["/notes.txt"]);
    let entry = apply(&f.repo(), &action).undo.unwrap();
    assert_eq!(
        f.repo().show("HEAD", "/notes.txt").unwrap().as_deref(),
        Some("staged\n")
    );
    assert_eq!(f.repo().show("HEAD", "/other.txt").unwrap(), None);
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(
        ref_id(f.dir(), "refs/heads/main").as_deref(),
        Some(head.as_str())
    );
    assert_eq!(f.read("notes.txt"), "staged\n");
    assert_eq!(f.read("other.txt"), "untouched\n");
}

#[test]
fn commit_action_makes_the_first_commit_and_undo_removes_it() {
    let dir = tmp("commit-first");
    new_repo(dir.path(), "Ada");
    write(dir.path(), "a.txt", "a\n");
    let action = commit("First", &["/a.txt"]);
    let p = plan(&reopen(dir.path()), &action);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(!p.backup, "nothing to back up before the first commit");
    assert_eq!(p.refs[0].before, None);
    assert!(
        p.steps.iter().any(|s| s.contains("first commit")),
        "{:?}",
        p.steps
    );
    let entry = apply(&reopen(dir.path()), &action).undo.unwrap();
    assert!(ref_id(dir.path(), "refs/heads/main").is_some());
    reopen(dir.path()).undo(&entry.id, false).unwrap();
    assert_eq!(ref_id(dir.path(), "refs/heads/main"), None);
    assert_eq!(read(dir.path(), "a.txt"), "a\n");
    if let Some(s) = porcelain(dir.path()) {
        assert_eq!(s.trim(), "?? a.txt");
    }
}

#[test]
fn commit_action_is_blocked_in_plain_words() {
    let f = fixture("commit-blocked");
    f.write("notes.txt", "changed\n");
    let blocked = |a: &GitAction| plan(&f.repo(), a).blocked.unwrap();
    let b = blocked(&commit("  ", &["/notes.txt"]));
    assert_eq!(b.reason, GitBlockReason::InvalidName);
    assert!(b.message.contains("message"), "{}", b.message);
    assert_eq!(
        blocked(&commit("m", &[])).reason,
        GitBlockReason::NothingToDo
    );
    assert_eq!(
        blocked(&commit("m", &["/project.yml"])).reason,
        GitBlockReason::NothingToDo
    );
    let staged = GitAction::Commit {
        message: "m".into(),
        paths: Vec::new(),
        staged: true,
    };
    assert_eq!(blocked(&staged).message, "Nothing is staged.");
    // Unsaved edits in the app block a commit of working files, not one of the index.
    let p = f.repo().plan(&commit("m", &["/notes.txt"]), true).unwrap();
    assert_eq!(p.blocked.unwrap().reason, GitBlockReason::UnsavedChanges);
    // Detached HEAD.
    let head = ref_id(f.dir(), "refs/heads/main").unwrap();
    apply(
        &f.repo(),
        &act(&format!(r#"{{"action":"switch","target":"{head}"}}"#)),
    );
    assert_eq!(
        blocked(&commit("m", &["/notes.txt"])).reason,
        GitBlockReason::Detached
    );
    // Apply refuses what the plan blocks, with its message, and changes nothing.
    let before = raw_state(f.dir());
    let e = f
        .repo()
        .apply(&commit("m", &["/notes.txt"]), &options(false))
        .unwrap_err();
    assert!(e.to_string().contains("not on a branch"), "{e}");
    assert_eq!(raw_state(f.dir()), before);
}

struct NoTokens;

impl CommitScanner for NoTokens {
    fn refusal(&self, files: &[(&str, &[u8])]) -> Option<String> {
        files
            .iter()
            .find(|(_, b)| b.windows(5).any(|w| w == b"TOKEN"))
            .map(|(p, _)| format!("a token in {p}"))
    }
}

#[test]
fn commit_action_stops_on_a_finding_of_the_scanner_before_anything_moves() {
    let f = fixture("commit-scan");
    f.write("notes.txt", "TOKEN=abc\n");
    let before = raw_state(f.dir());
    let repo = f.repo().with_scanner(Some(Arc::new(NoTokens)));
    let e = repo
        .apply(&commit("m", &["/notes.txt"]), &options(false))
        .unwrap_err();
    assert!(
        matches!(e, tk_git::GitError::SecretsFound(ref m) if m.contains("notes.txt")),
        "{e:?}"
    );
    assert_eq!(raw_state(f.dir()), before);
    // No backup ref either.
    assert!(!state(f.dir()).contains("refs/tailor/backup"));
    let staged = f.repo().with_scanner(Some(Arc::new(NoTokens)));
    staged.stage(&paths(&["/notes.txt"])).unwrap();
    let e = staged
        .apply(
            &GitAction::Commit {
                message: "m".into(),
                paths: Vec::new(),
                staged: true,
            },
            &options(false),
        )
        .unwrap_err();
    assert!(matches!(e, tk_git::GitError::SecretsFound(_)), "{e:?}");
}

#[test]
fn commit_action_concludes_a_merge_with_two_parents() {
    let f = fixture("commit-merge");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#),
    );
    let theirs = f.commit("theirs", &[("notes.txt", "theirs\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    let ours = f.commit("ours", &[("notes.txt", "ours\n")]);
    apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    assert!(f.repo().status().unwrap().merging);
    // A conflicted file not named blocks.
    let p = plan(&f.repo(), &commit("Merge other", &[]));
    assert_eq!(p.blocked.unwrap().reason, GitBlockReason::InProgress);
    f.write("notes.txt", "both\n");
    let action = commit("Merge other", &["/notes.txt"]);
    let p = plan(&f.repo(), &action);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.steps[0].contains("Finishes the merge"), "{:?}", p.steps);
    assert!(!p.warnings.is_empty());
    let entry = apply(&f.repo(), &action).undo.unwrap();
    let info = f.repo().commit_info("HEAD").unwrap();
    assert_eq!(info.parents, [ours.clone(), theirs]);
    assert!(!f.repo().status().unwrap().merging);
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(
        ref_id(f.dir(), "refs/heads/main").as_deref(),
        Some(ours.as_str())
    );
    assert_eq!(f.read("notes.txt"), "both\n");
}

#[test]
fn commit_action_undo_is_refused_once_the_branch_moved() {
    let f = fixture("commit-stale");
    f.write("notes.txt", "one more\n");
    let entry = apply(&f.repo(), &commit("first", &["/notes.txt"]))
        .undo
        .unwrap();
    f.commit("second", &[("loose.txt", "x\n")]);
    let last = f.repo().last_action().unwrap().unwrap();
    assert_eq!(last.id, entry.id);
    assert!(last.stale.is_some());
    let e = f.repo().undo(&entry.id, false).unwrap_err();
    assert!(e.to_string().contains("Cannot undo"), "{e}");
}

#[test]
fn commit_action_signs_through_the_repository_signer_and_takes_the_chosen_author() {
    let f = fixture("commit-author");
    f.write("notes.txt", "by the profile\n");
    let chosen = GitIdentity {
        name: "Profile Person".into(),
        email: "profile@example.com".into(),
    };
    let repo = f.repo().with_author(Some(chosen.clone()));
    apply(&repo, &commit("m", &["/notes.txt"]));
    let info = f.repo().commit_info("HEAD").unwrap();
    assert_eq!(info.author.name, "Profile Person");
    assert_eq!(info.author.email, "profile@example.com");
    assert_eq!(info.committer.name, "Profile Person");
    // The Git configuration of the repository (Ada) was not used and not written.
    assert!(!read(&f.dir().join(".git"), "config").contains("Profile Person"));
    // Without an author, the Git configuration counts as before.
    f.write("notes.txt", "by ada\n");
    apply(&f.repo(), &commit("m2", &["/notes.txt"]));
    assert_eq!(f.repo().commit_info("HEAD").unwrap().author.name, "Ada");
}

// ---- push undo (`with_push_undo`) ------------------------------------------------------------

fn pushing(dir: &std::path::Path) -> Repo {
    reopen(dir).with_push_undo(true)
}

#[test]
fn push_is_undone_with_a_lease_when_the_product_opts_in() {
    let f = fixture("push-undo");
    let base = ref_id(f.remote.path(), "refs/heads/main").unwrap();
    let ahead = f.commit("ahead", &[("notes.txt", "ahead\n")]);
    // Without the option, nothing changes for the products that did not opt in.
    assert!(!plan(&f.repo(), &act(r#"{"action":"push"}"#)).undoable);

    let p = plan(&pushing(f.dir()), &act(r#"{"action":"push"}"#));
    assert!(p.undoable && p.network && !p.backup, "{p:?}");
    assert!(
        p.steps[0].contains("Undo can take them off"),
        "{:?}",
        p.steps
    );
    let entry = apply(&pushing(f.dir()), &act(r#"{"action":"push"}"#))
        .undo
        .expect("an undoable push");
    assert_eq!(entry.action, "push");
    assert_eq!(
        ref_id(f.remote.path(), "refs/heads/main").as_deref(),
        Some(ahead.as_str())
    );
    assert!(
        pushing(f.dir())
            .last_action()
            .unwrap()
            .unwrap()
            .stale
            .is_none()
    );

    assert_eq!(
        pushing(f.dir()).pushed_ref(&entry.id).unwrap(),
        Some(("origin".to_owned(), "refs/heads/main".to_owned()))
    );
    pushing(f.dir())
        .undo_with(&entry.id, false, &NoCredentials)
        .unwrap();
    assert_eq!(
        ref_id(f.remote.path(), "refs/heads/main").as_deref(),
        Some(base.as_str())
    );
    assert_eq!(
        ref_id(f.dir(), "refs/remotes/origin/main").as_deref(),
        Some(base.as_str())
    );
    // The local branch keeps the commit.
    assert_eq!(
        ref_id(f.dir(), "refs/heads/main").as_deref(),
        Some(ahead.as_str())
    );
    if let Some(out) = git(f.remote.path(), &["rev-parse", "refs/heads/main"]) {
        assert_eq!(out.trim(), base);
    }
}

#[test]
fn push_undo_is_refused_once_someone_pushed_on_top() {
    let f = fixture("push-undo-stale");
    f.commit("ahead", &[("notes.txt", "ahead\n")]);
    let entry = apply(&pushing(f.dir()), &act(r#"{"action":"push"}"#))
        .undo
        .unwrap();
    // Somebody else pushes on top (a second clone).
    let other = tmp("push-undo-other");
    let theirs_dir = tk_git::clone(
        f.remote.path().to_str().unwrap(),
        other.path(),
        &NoCredentials,
        &opts(),
    )
    .unwrap();
    set_user(&theirs_dir, "Bob");
    write(&theirs_dir, "notes.txt", "bob\n");
    let theirs = reopen(&theirs_dir)
        .commit("bob", &paths(&["notes.txt"]), None)
        .unwrap()
        .oid;
    reopen(&theirs_dir).push(&NoCredentials).unwrap();
    let remote_before = ref_id(f.remote.path(), "refs/heads/main");
    assert_eq!(remote_before.as_deref(), Some(theirs.as_str()));
    // The local view does not know yet: undo asks the server, which refuses.
    let e = pushing(f.dir())
        .undo_with(&entry.id, false, &NoCredentials)
        .unwrap_err();
    assert!(e.to_string().contains("revert"), "{e}");
    assert_eq!(ref_id(f.remote.path(), "refs/heads/main"), remote_before);
    // After a fetch, the entry says it is stale before anything is tried.
    apply(&pushing(f.dir()), &act(r#"{"action":"fetch"}"#));
    let last = pushing(f.dir()).last_action().unwrap().unwrap();
    assert!(last.stale.unwrap().contains("server changed"));
}

#[test]
fn publish_is_undone_by_removing_the_branch_from_the_remote() {
    let f = fixture("publish-undo");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"feature","start":"HEAD","switch":true}"#),
    );
    let tip = f.commit("feature", &[("notes.txt", "feature\n")]);
    let config = read(&f.dir().join(".git"), "config");
    let entry = apply(
        &pushing(f.dir()),
        &act(r#"{"action":"publish","branch":"feature","remote":"origin"}"#),
    )
    .undo
    .unwrap();
    assert_eq!(
        ref_id(f.remote.path(), "refs/heads/feature").as_deref(),
        Some(tip.as_str())
    );
    pushing(f.dir())
        .undo_with(&entry.id, false, &NoCredentials)
        .unwrap();
    assert_eq!(ref_id(f.remote.path(), "refs/heads/feature"), None);
    assert_eq!(ref_id(f.dir(), "refs/remotes/origin/feature"), None);
    assert_eq!(read(&f.dir().join(".git"), "config"), config);
}

#[test]
fn push_undo_works_over_smart_http() {
    let Some(server) = git_http() else {
        eprintln!("skipped: no git http-backend");
        return;
    };
    let name = format!("push-undo-{}.git", std::process::id());
    let remote = server.root.join(&name);
    std::fs::create_dir_all(&remote).unwrap();
    bare(&remote);
    let work = tmp("push-undo-http");
    new_repo(work.path(), "Ada");
    add_remote(work.path(), &server.url(&name, false));
    write(work.path(), "a.txt", "a\n");
    let first = reopen(work.path())
        .commit("one", &paths(&["a.txt"]), None)
        .unwrap()
        .oid;
    reopen(work.path()).push(&NoCredentials).unwrap();
    write(work.path(), "a.txt", "b\n");
    let second = reopen(work.path())
        .commit("two", &paths(&["a.txt"]), None)
        .unwrap()
        .oid;
    let opts_push = ActionOptions {
        unsaved: false,
        credentials: &NoCredentials,
        identity: None,
    };
    let entry = pushing(work.path())
        .apply(&act(r#"{"action":"push"}"#), &opts_push)
        .unwrap()
        .undo
        .unwrap();
    assert_eq!(
        ref_id(&remote, "refs/heads/main").as_deref(),
        Some(second.as_str())
    );
    pushing(work.path())
        .undo_with(&entry.id, false, &NoCredentials)
        .unwrap();
    assert_eq!(
        ref_id(&remote, "refs/heads/main").as_deref(),
        Some(first.as_str())
    );
}

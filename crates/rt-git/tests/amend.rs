// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Amend (wave 15, strand D): the last commit is replaced with the message and files asked
//! for, keeping parents and author; the plan says so in plain words, a backup ref keeps the
//! old commit, Undo puts everything back; a commit that is on a remote is refused
//! (`published`), as are unsaved changes and a merge in progress.

mod common;
mod wizard;

use common::*;
use rt_app::git::actions::GitBlockReason;
use wizard::*;

fn have_git() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok()
}

#[test]
fn amends_the_message_and_keeps_parents_and_author() {
    let f = fixture("amend-message");
    let old = f.commit("secnd", &[("notes.txt", "two\n")]);
    let before_info = f.repo().commit_info("HEAD").unwrap();
    let before = state(f.dir());
    let action = act(r#"{"action":"amend","message":"second"}"#);
    let p = plan(&f.repo(), &action);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.backup && p.undoable && !p.destructive);
    assert!(
        p.steps
            .iter()
            .any(|s| s.contains("second") && s.contains("secnd")),
        "{:?}",
        p.steps
    );
    assert_eq!(p.refs.len(), 1);
    assert_eq!(p.refs[0].name, "refs/heads/main");
    assert_eq!(p.refs[0].before.as_deref(), Some(old.as_str()));
    assert_eq!(p.refs[0].after.as_deref(), Some(""));
    assert_eq!(raw_state(f.dir()), {
        let _ = plan(&f.repo(), &action);
        raw_state(f.dir())
    });

    let r = apply(&f.repo(), &action);
    let entry = r.undo.unwrap();
    assert!(!r.reload);
    assert_eq!(entry.action, "amend");
    assert_eq!(entry.backups.len(), 1);
    assert_eq!(
        ref_id(f.dir(), &entry.backups[0]).as_deref(),
        Some(old.as_str())
    );
    let after = f.repo().commit_info("HEAD").unwrap();
    assert_ne!(after.oid, old);
    assert_eq!(after.message.trim(), "second");
    assert_eq!(after.parents, before_info.parents);
    assert_eq!(after.tree, before_info.tree);
    assert_eq!(after.author, before_info.author);
    assert_eq!(f.read("notes.txt"), "two\n");
    assert!(f.repo().status().unwrap().files.is_empty());

    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(state(f.dir()), before);
}

#[test]
fn amends_the_current_content_of_the_files_named_only() {
    let f = fixture("amend-files");
    f.commit("second", &[("notes.txt", "two\n")]);
    f.write("notes.txt", "three\n");
    f.write(
        "users/get.yml",
        &request("POST", "https://api.test/users/1"),
    );
    f.write("loose.txt", "untracked\n");
    let action = act(r#"{"action":"amend","paths":["/notes.txt","/loose.txt"]}"#);
    let p = plan(&f.repo(), &action);
    assert_eq!(p.files, paths(&["/loose.txt", "/notes.txt"]));
    let before = state(f.dir());
    let r = apply(&f.repo(), &action);
    let entry = r.undo.unwrap();
    let info = f.repo().commit_info("HEAD").unwrap();
    assert_eq!(info.message.trim(), "second", "the message stays");
    assert_eq!(
        f.repo().show("HEAD", "/notes.txt").unwrap().as_deref(),
        Some("three\n")
    );
    assert_eq!(
        f.repo().show("HEAD", "/loose.txt").unwrap().as_deref(),
        Some("untracked\n")
    );
    assert_eq!(
        f.repo().show("HEAD", "/users/get.yml").unwrap().unwrap(),
        request("GET", "https://api.test/users/1"),
        "files not named stay as they were committed"
    );
    // The rest still shows as a change; the files are untouched.
    let left: Vec<String> = f
        .repo()
        .status()
        .unwrap()
        .files
        .iter()
        .map(|x| x.path.clone())
        .collect();
    assert_eq!(left, vec!["users/get.yml".to_owned()]);
    assert_eq!(f.read("notes.txt"), "three\n");
    if have_git() {
        let out = git(f.dir(), &["status", "--porcelain"]).unwrap();
        assert_eq!(out.trim(), "M users/get.yml");
    }
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(state(f.dir()), before);
}

#[test]
fn amends_what_the_index_holds() {
    if !have_git() {
        return;
    }
    let f = fixture("amend-staged");
    f.commit("second", &[("notes.txt", "two\n")]);
    f.write("notes.txt", "staged\n");
    f.write(
        "users/get.yml",
        &request("POST", "https://api.test/users/1"),
    );
    git(f.dir(), &["add", "notes.txt"]).unwrap();
    let staged_tree = git(f.dir(), &["write-tree"]).unwrap();
    apply(&f.repo(), &act(r#"{"action":"amend","staged":true}"#));
    assert_eq!(
        f.repo().commit_info("HEAD").unwrap().tree,
        staged_tree.trim()
    );
    let out = git(f.dir(), &["status", "--porcelain"]).unwrap();
    assert_eq!(out.trim(), "M users/get.yml");
}

#[test]
fn refuses_published_commits_and_what_cannot_run() {
    let f = fixture("amend-blocked");
    // "init" is on the remote.
    let a = act(r#"{"action":"amend","message":"nope"}"#);
    let p = plan(&f.repo(), &a);
    let b = p.blocked.unwrap();
    assert_eq!(b.reason, GitBlockReason::Published);
    assert!(b.message.contains("server"), "{}", b.message);
    let before = raw_state(f.dir());
    let err = f.repo().apply(&a, &options(false)).unwrap_err();
    assert!(err.to_string().contains("server"), "{err}");
    assert_eq!(raw_state(f.dir()), before);

    f.commit("local", &[("notes.txt", "local\n")]);
    assert_eq!(reason(&f.repo(), &a), None);
    // Unsaved edits matter only when files are taken from disk.
    assert!(f.repo().plan(&a, true).unwrap().blocked.is_none());
    assert_eq!(
        f.repo()
            .plan(&act(r#"{"action":"amend","paths":["/notes.txt"]}"#), true)
            .unwrap()
            .blocked
            .unwrap()
            .reason,
        GitBlockReason::UnsavedChanges
    );
    // Nothing to change.
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"amend"}"#)),
        Some(GitBlockReason::NothingToDo)
    );
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"amend","message":"local"}"#)),
        Some(GitBlockReason::NothingToDo)
    );
    assert_eq!(
        reason(
            &f.repo(),
            &act(r#"{"action":"amend","paths":["/notes.txt"]}"#)
        ),
        Some(GitBlockReason::NothingToDo)
    );
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"amend","message":"  "}"#)),
        Some(GitBlockReason::InvalidName)
    );
    // Pushed: no longer amendable.
    f.repo().push(&rt_git::NoCredentials).unwrap();
    assert_eq!(reason(&f.repo(), &a), Some(GitBlockReason::Published));
}

#[test]
fn refuses_while_a_merge_is_waiting() {
    let f = fixture("amend-merging");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#),
    );
    f.commit("theirs", &[("notes.txt", "theirs\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    f.commit("ours", &[("notes.txt", "ours\n")]);
    apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"amend","message":"x"}"#)),
        Some(GitBlockReason::InProgress)
    );
}

#[test]
fn undo_is_refused_once_the_branch_moved_on() {
    let f = fixture("amend-stale");
    f.commit("second", &[("notes.txt", "two\n")]);
    let r = apply(&f.repo(), &act(r#"{"action":"amend","message":"renamed"}"#));
    f.commit("third", &[("notes.txt", "three\n")]);
    let err = f.repo().undo(&r.undo.unwrap().id, false).unwrap_err();
    assert!(err.to_string().contains("Cannot undo"), "{err}");
}

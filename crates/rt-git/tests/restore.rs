// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Restore and park (wave 15, strand D): files come back from any commit (a path missing there
//! is deleted), a branch that is not checked out points at a commit again (also a deleted
//! one, e.g. from the reflog), and "park changes" can take just some files. Each has a plan in
//! plain words, a backup where work is replaced, and Undo.

mod common;
mod wizard;

use common::*;
use rt_app::git::actions::GitBlockReason;
use wizard::*;

fn have_git() -> bool {
    std::process::Command::new("git").arg("--version").output().is_ok()
}

/// "init", then "second" (changes the notes and the request, adds `extra.txt`).
fn two_commits(name: &str) -> Fixture {
    let f = fixture(name);
    f.commit(
        "second",
        &[
            ("notes.txt", "second\n"),
            ("users/get.yml", &request("POST", "https://api.test/users/1")),
            ("extra.txt", "extra\n"),
        ],
    );
    f
}

#[test]
fn restores_files_from_an_earlier_commit() {
    let f = two_commits("restore-files");
    f.write("notes.txt", "my edit\n");
    let before = state(f.dir());
    let a = act(r#"{"action":"restore-files","rev":"HEAD~1","paths":["/notes.txt","/extra.txt","/users"]}"#);
    let p = plan(&f.repo(), &a);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.backup && p.undoable && p.destructive, "uncommitted work is replaced");
    assert_eq!(p.files, paths(&["/extra.txt", "/notes.txt", "/users/get.yml"]));
    assert!(p.warnings.iter().any(|w| w.contains("uncommitted")), "{:?}", p.warnings);
    assert_eq!(raw_state(f.dir()), {
        let _ = plan(&f.repo(), &a);
        raw_state(f.dir())
    }, "a plan changes nothing");

    let r = apply(&f.repo(), &a);
    assert!(r.reload);
    let entry = r.undo.unwrap();
    assert!(entry.backups.iter().any(|b| b.ends_with("/worktree")), "{:?}", entry.backups);
    assert_eq!(f.read("notes.txt"), NOTES);
    assert_eq!(f.read("users/get.yml"), request("GET", "https://api.test/users/1"));
    assert!(!f.exists("extra.txt"), "missing at that commit: deleted");
    // The branch did not move; the files show as changes against HEAD.
    assert_eq!(f.repo().rev_parse("HEAD").unwrap(), f.repo().rev_parse("main").unwrap());
    if have_git() {
        let diff = git(f.dir(), &["diff", "--stat", "HEAD~1", "--", "notes.txt", "users", "extra.txt"]).unwrap();
        assert!(diff.trim().is_empty(), "files equal HEAD~1: {diff}");
    }
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(state(f.dir()), before);
    assert_eq!(f.read("notes.txt"), "my edit\n");
}

#[test]
fn restoring_the_same_files_is_nothing_to_do_and_refuses_what_cannot_run() {
    let f = two_commits("restore-files-blocked");
    for (json, want) in [
        (r#"{"action":"restore-files","rev":"HEAD","paths":["/notes.txt"]}"#, GitBlockReason::NothingToDo),
        (r#"{"action":"restore-files","rev":"HEAD","paths":[]}"#, GitBlockReason::NothingToDo),
        (r#"{"action":"restore-files","rev":"nope","paths":["/notes.txt"]}"#, GitBlockReason::NotFound),
    ] {
        assert_eq!(reason(&f.repo(), &act(json)), Some(want), "{json}");
    }
    let a = act(r#"{"action":"restore-files","rev":"HEAD~1","paths":["/notes.txt"]}"#);
    assert_eq!(reason(&f.repo(), &a), None);
    assert_eq!(
        f.repo().plan(&a, true).unwrap().blocked.unwrap().reason,
        GitBlockReason::UnsavedChanges
    );
    // Collection-relative paths work too.
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"restore-files","rev":"HEAD~1","paths":["notes.txt"]}"#)),
        None
    );
}

#[test]
fn restores_a_deleted_branch_from_where_it_was() {
    let f = two_commits("restore-branch");
    let tip = f.repo().rev_parse("HEAD").unwrap();
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"feature","start":"HEAD","switch":false}"#),
    );
    apply(&f.repo(), &act(r#"{"action":"delete-branch","name":"feature","force":true}"#));
    assert!(ref_id(f.dir(), "refs/heads/feature").is_none());
    let before = state(f.dir());
    let a = act(&format!(r#"{{"action":"restore-branch","name":"feature","target":"{tip}"}}"#));
    let p = plan(&f.repo(), &a);
    assert!(p.blocked.is_none());
    assert!(p.undoable && !p.destructive);
    assert!(p.steps[0].contains("feature") && p.steps[0].contains("again"), "{:?}", p.steps);
    assert_eq!(p.refs[0].before, None);
    let r = apply(&f.repo(), &a);
    assert!(!r.reload);
    assert_eq!(ref_id(f.dir(), "refs/heads/feature").as_deref(), Some(tip.as_str()));
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(state(f.dir()), before);
}

#[test]
fn moves_a_branch_that_exists_with_a_backup_and_refuses_the_current_one() {
    let f = two_commits("restore-branch-move");
    let first = f.repo().rev_parse("HEAD~1").unwrap();
    let tip = f.repo().rev_parse("HEAD").unwrap();
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"feature","start":"HEAD","switch":false}"#),
    );
    let a = act(&format!(r#"{{"action":"restore-branch","name":"feature","target":"{first}"}}"#));
    let p = plan(&f.repo(), &a);
    assert!(p.backup);
    assert!(p.warnings.iter().any(|w| w.contains("commit leaves")), "{:?}", p.warnings);
    let r = apply(&f.repo(), &a);
    let entry = r.undo.unwrap();
    assert_eq!(entry.backups.len(), 1);
    assert_eq!(ref_id(f.dir(), &entry.backups[0]).as_deref(), Some(tip.as_str()));
    assert_eq!(ref_id(f.dir(), "refs/heads/feature").as_deref(), Some(first.as_str()));
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(ref_id(f.dir(), "refs/heads/feature").as_deref(), Some(tip.as_str()));

    for (json, want) in [
        (format!(r#"{{"action":"restore-branch","name":"main","target":"{first}"}}"#), GitBlockReason::CurrentBranch),
        (format!(r#"{{"action":"restore-branch","name":"feature","target":"{tip}"}}"#), GitBlockReason::NothingToDo),
        (r#"{"action":"restore-branch","name":"bad name","target":"HEAD"}"#.to_owned(), GitBlockReason::InvalidName),
        (r#"{"action":"restore-branch","name":"x","target":"nope"}"#.to_owned(), GitBlockReason::NotFound),
    ] {
        assert_eq!(reason(&f.repo(), &act(&json)), Some(want), "{json}");
    }
}

#[test]
fn restores_a_branch_from_a_reflog_position_and_a_backup_ref() {
    let f = two_commits("restore-branch-reflog");
    let tip = f.repo().rev_parse("HEAD").unwrap();
    let r = apply(&f.repo(), &act(r#"{"action":"reset","target":"HEAD~1","mode":"keep-changes"}"#));
    let backup = r.undo.unwrap().backups[0].clone();
    // The backup ref names the lost commit.
    apply(
        &f.repo(),
        &act(&format!(r#"{{"action":"restore-branch","name":"rescued","target":"{backup}"}}"#)),
    );
    assert_eq!(ref_id(f.dir(), "refs/heads/rescued").as_deref(), Some(tip.as_str()));
    // And a reflog expression.
    apply(
        &f.repo(),
        &act(r#"{"action":"restore-branch","name":"again","target":"main@{1}"}"#),
    );
    assert_eq!(ref_id(f.dir(), "refs/heads/again").as_deref(), Some(tip.as_str()));
}

#[test]
fn parks_only_the_files_named() {
    let f = fixture("stash-partial");
    f.write("notes.txt", "parked\n");
    f.write("users/get.yml", &request("POST", "https://api.test/users/1"));
    f.write("loose.txt", "untracked\n");
    f.write("users/new.yml", &request("GET", "https://api.test/new"));
    let a = act(r#"{"action":"stash","message":"some","paths":["/notes.txt","/loose.txt"]}"#);
    let p = plan(&f.repo(), &a);
    assert_eq!(p.files, paths(&["/loose.txt", "/notes.txt"]));
    assert!(p.steps[0].contains("other changes stay"), "{:?}", p.steps);
    let before = state(f.dir());
    let r = apply(&f.repo(), &a);
    assert!(r.reload);
    assert_eq!(f.read("notes.txt"), NOTES);
    assert!(!f.exists("loose.txt"));
    assert_eq!(f.read("users/get.yml"), request("POST", "https://api.test/users/1"), "not named: stays");
    assert!(f.exists("users/new.yml"));
    let left: Vec<String> = f.repo().status().unwrap().files.iter().map(|x| x.path.clone()).collect();
    assert_eq!(left, vec!["users/get.yml".to_owned(), "users/new.yml".to_owned()]);
    let list = f.repo().stashes().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].files, paths(&["/loose.txt", "/notes.txt"]));
    if have_git() {
        let names = git(f.dir(), &["show", "--name-only", "--format=", "stash@{0}"]).unwrap();
        assert_eq!(names.trim(), "notes.txt");
        let untracked = git(f.dir(), &["show", "--name-only", "--format=", "stash@{0}^3"]).unwrap();
        assert_eq!(untracked.trim(), "loose.txt");
    }
    // Bring them back.
    apply(&f.repo(), &act(r#"{"action":"unstash","index":0}"#));
    assert_eq!(f.read("notes.txt"), "parked\n");
    assert_eq!(f.read("loose.txt"), "untracked\n");
    // Undo of the partial park too.
    let f2 = fixture("stash-partial-undo");
    f2.write("notes.txt", "parked\n");
    f2.write("users/get.yml", &request("POST", "https://api.test/users/1"));
    let before2 = state(f2.dir());
    let r2 = apply(&f2.repo(), &act(r#"{"action":"stash","paths":["/notes.txt"]}"#));
    f2.repo().undo(&r2.undo.unwrap().id, false).unwrap();
    assert_eq!(state(f2.dir()), before2);
    let _ = before;
}

#[test]
fn parking_files_without_changes_is_nothing_to_do() {
    let f = fixture("stash-partial-nothing");
    f.write("notes.txt", "changed\n");
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"stash","paths":["/users/get.yml"]}"#)),
        Some(GitBlockReason::NothingToDo)
    );
    // A folder takes everything under it.
    f.write("users/get.yml", &request("POST", "https://api.test/users/1"));
    let p = plan(&f.repo(), &act(r#"{"action":"stash","paths":["/users"]}"#));
    assert_eq!(p.files, paths(&["/users/get.yml"]));
}

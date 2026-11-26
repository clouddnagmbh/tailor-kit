// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Authors

//! "Park changes" is a real Git stash (wave 14, strand G): `refs/stash` and its reflog, so
//! `git stash list` (when `git` is installed) sees the same list; untracked files are parked
//! too; bringing back merges onto a moved branch, refuses to overwrite local changes, and
//! `keep` leaves the entry; dropping a middle entry rewrites the list like Git.

mod common;
mod wizard;

use common::*;
use tk_git_types::actions::GitBlockReason;
use wizard::*;

#[test]
fn parks_and_brings_back_tracked_and_untracked_files() {
    let f = fixture("stash-basic");
    f.write("notes.txt", "parked\n");
    f.write("users/new.yml", &http_doc("GET", "https://api.test/new"));
    let p = plan(
        &f.repo(),
        &act(r#"{"action":"stash","message":"try health"}"#),
    );
    assert_eq!(p.files, paths(&["/notes.txt", "/users/new.yml"]));
    assert!(!p.backup && p.undoable);
    let r = apply(&f.repo(), &p.action);
    assert!(r.reload);
    assert!(f.repo().status().unwrap().files.is_empty());
    assert_eq!(f.read("notes.txt"), NOTES);
    assert!(!f.exists("users/new.yml"));

    let list = f.repo().stashes().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].index, 0);
    assert_eq!(list[0].message, "try health");
    assert_eq!(list[0].branch.as_deref(), Some("main"));
    assert_eq!(list[0].files, paths(&["/notes.txt", "/users/new.yml"]));
    assert_eq!(ref_id(f.dir(), "refs/stash"), Some(list[0].oid.clone()));

    apply(&f.repo(), &act(r#"{"action":"unstash","index":0}"#));
    assert_eq!(f.read("notes.txt"), "parked\n");
    assert_eq!(
        f.read("users/new.yml"),
        http_doc("GET", "https://api.test/new")
    );
    assert!(f.repo().stashes().unwrap().is_empty());
    assert!(ref_id(f.dir(), "refs/stash").is_none());
}

#[test]
fn git_sees_the_same_stash_list() {
    let f = fixture("stash-git");
    for (n, text) in ["first", "second", "third"].iter().enumerate() {
        f.write("notes.txt", &format!("{text}\n"));
        if n == 1 {
            f.write("untracked.txt", "u\n");
        }
        apply(
            &f.repo(),
            &act(&format!(r#"{{"action":"stash","message":"{text}"}}"#)),
        );
    }
    let ours: Vec<String> = f
        .repo()
        .stashes()
        .unwrap()
        .into_iter()
        .map(|s| s.message)
        .collect();
    assert_eq!(ours, ["third", "second", "first"]);
    let Some(list) = git(f.dir(), &["stash", "list"]) else {
        eprintln!("SKIPPED: `git` is not installed");
        return;
    };
    assert_eq!(
        list.lines().collect::<Vec<_>>(),
        [
            "stash@{0}: On main: third",
            "stash@{1}: On main: second",
            "stash@{2}: On main: first"
        ]
    );
    // Git can read what we parked, untracked files included.
    let shown = git(
        f.dir(),
        &[
            "stash",
            "show",
            "--include-untracked",
            "--name-only",
            "stash@{1}",
        ],
    )
    .unwrap();
    assert!(
        shown.contains("notes.txt") && shown.contains("untracked.txt"),
        "{shown}"
    );

    // Drop the middle one; Git agrees and the others still apply.
    apply(&f.repo(), &act(r#"{"action":"drop-stash","index":1}"#));
    let list = git(f.dir(), &["stash", "list"]).unwrap();
    assert_eq!(
        list.lines().collect::<Vec<_>>(),
        ["stash@{0}: On main: third", "stash@{1}: On main: first"]
    );
    apply(&f.repo(), &act(r#"{"action":"unstash","index":1}"#));
    assert_eq!(f.read("notes.txt"), "first\n");
    let list = git(f.dir(), &["stash", "list"]).unwrap();
    assert_eq!(
        list.lines().collect::<Vec<_>>(),
        ["stash@{0}: On main: third"]
    );
    // A stash made by Git shows in the app.
    std::fs::remove_file(f.dir().join("notes.txt")).unwrap();
    git(
        f.dir(),
        &[
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=ada@example.com",
            "stash",
            "push",
            "-m",
            "by git",
        ],
    )
    .unwrap();
    let ours = f.repo().stashes().unwrap();
    assert_eq!(ours[0].message, "by git");
    assert_eq!(ours[0].files, paths(&["/notes.txt"]));
}

#[test]
fn keep_leaves_it_in_the_list() {
    let f = fixture("stash-keep");
    f.write("notes.txt", "parked\n");
    apply(&f.repo(), &act(r#"{"action":"stash"}"#));
    assert_eq!(f.repo().stashes().unwrap()[0].message, "Parked on main");
    let p = plan(
        &f.repo(),
        &act(r#"{"action":"unstash","index":0,"keep":true}"#),
    );
    assert!(p.refs.is_empty(), "{:?}", p.refs);
    apply(&f.repo(), &p.action);
    assert_eq!(f.read("notes.txt"), "parked\n");
    assert_eq!(f.repo().stashes().unwrap().len(), 1);
}

#[test]
fn brings_back_onto_a_moved_branch_and_refuses_to_overwrite() {
    let f = fixture("stash-moved");
    f.write(
        "notes.txt",
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nPARKED\n",
    );
    apply(&f.repo(), &act(r#"{"action":"stash"}"#));
    f.commit(
        "later",
        &[(
            "notes.txt",
            "LATER\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
        )],
    );
    apply(
        &f.repo(),
        &act(r#"{"action":"unstash","index":0,"keep":true}"#),
    );
    assert_eq!(
        f.read("notes.txt"),
        "LATER\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nPARKED\n"
    );

    // Local changes to a parked file are not overwritten.
    f.write("notes.txt", "mine\n");
    let p = plan(&f.repo(), &act(r#"{"action":"unstash","index":0}"#));
    assert_eq!(
        p.blocked.map(|b| b.reason),
        Some(GitBlockReason::LocalChanges)
    );

    // Overlapping changes since: the plan says so instead of making a mess.
    f.write(
        "notes.txt",
        "LATER\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    );
    f.commit(
        "clash",
        &[(
            "notes.txt",
            "LATER\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nCLASH\n",
        )],
    );
    let p = plan(&f.repo(), &act(r#"{"action":"unstash","index":0}"#));
    let b = p.blocked.unwrap();
    assert_eq!(b.reason, GitBlockReason::Unsupported);
    assert!(b.message.contains("git stash apply"), "{}", b.message);
}

#[test]
fn nothing_to_park_and_no_such_entry() {
    let f = fixture("stash-none");
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"stash"}"#)),
        Some(GitBlockReason::NothingToDo)
    );
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"drop-stash","index":0}"#)),
        Some(GitBlockReason::NotFound)
    );
    f.write("notes.txt", "x\n");
    assert_eq!(
        f.repo()
            .plan(&act(r#"{"action":"stash"}"#), true)
            .unwrap()
            .blocked
            .map(|b| b.reason),
        Some(GitBlockReason::UnsavedChanges)
    );
}

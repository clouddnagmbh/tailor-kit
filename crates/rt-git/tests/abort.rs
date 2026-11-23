// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Abort (wave 15, strand D): a merge, cherry-pick or revert that stopped on conflicts, or a
//! rebase started on the command line, is stopped and the repository is where it started
//! (the state `git merge --abort` / `git rebase --abort` leave); files Git does not know stay;
//! plan in plain words, backup ref, Undo brings the files back (not the operation).

mod common;
mod wizard;

use rt_app::git::actions::GitBlockReason;
use wizard::*;

fn have_git() -> bool {
    std::process::Command::new("git").arg("--version").output().is_ok()
}

/// `main` and `other` both change line five of the notes; `other` also adds a request.
fn conflicting(name: &str) -> Fixture {
    let f = fixture(name);
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#),
    );
    f.commit(
        "theirs",
        &[
            ("notes.txt", &NOTES.replacen("five", "5 theirs", 1)),
            ("users/added.yml", &request("GET", "https://api.test/added")),
        ],
    );
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    f.commit("ours", &[("notes.txt", &NOTES.replacen("five", "5 ours", 1))]);
    f
}

#[test]
fn aborts_a_merge_back_to_where_it_started() {
    let f = conflicting("abort-merge");
    let before = state(f.dir());
    apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    assert!(f.exists("users/added.yml"), "the merge brought a file");
    f.write("loose.txt", "not known to Git\n");
    f.repo()
        .resolve("notes.txt", rt_app::git::GitResolution::Theirs)
        .unwrap();

    let a = act(r#"{"action":"abort"}"#);
    let p = plan(&f.repo(), &a);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.backup && p.undoable && p.destructive);
    assert!(p.steps[0].contains("merge"), "{:?}", p.steps);
    assert!(p.files.contains(&"/notes.txt".to_owned()));
    let r = apply(&f.repo(), &a);
    assert!(r.reload);
    let entry = r.undo.unwrap();
    assert_eq!(entry.action, "abort");
    assert!(entry.backups.iter().any(|b| b.ends_with("/worktree")), "{:?}", entry.backups);

    assert!(!f.dir().join(".git/MERGE_HEAD").exists());
    assert!(!f.repo().status().unwrap().merging);
    assert!(!f.exists("users/added.yml"));
    assert_eq!(f.read("notes.txt"), NOTES.replacen("five", "5 ours", 1));
    assert_eq!(f.read("loose.txt"), "not known to Git\n", "unknown files stay");
    std::fs::remove_file(f.dir().join("loose.txt")).unwrap();
    assert_eq!(state(f.dir()), before);
    if have_git() {
        let (out, _) = (git(f.dir(), &["status", "--porcelain"]).unwrap(), 0);
        assert!(out.trim().is_empty(), "{out}");
    }
}

#[test]
fn undo_brings_the_files_back_as_changes() {
    let f = conflicting("abort-undo");
    apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    let during = files(f.dir());
    assert!(f.read("notes.txt").contains("<<<<<<<"));
    let r = apply(&f.repo(), &act(r#"{"action":"abort"}"#));
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(files(f.dir()), during, "the files are back, markers and all");
    assert!(!f.repo().status().unwrap().merging, "the merge itself is not resumed");
}

#[test]
fn aborts_a_cherry_pick_and_a_revert() {
    for (name, action) in [
        ("pick", r#"{"action":"cherry-pick","commit":"other"}"#),
        ("revert", r#"{"action":"revert","commit":"HEAD~1"}"#),
    ] {
        let f = conflicting(&format!("abort-{name}"));
        if name == "revert" {
            // Make the commit to take back conflict with a later one.
            f.commit("later", &[("notes.txt", &NOTES.replacen("five", "5 later", 1))]);
        }
        let before = state(f.dir());
        apply(&f.repo(), &act(action));
        assert!(f.repo().status().unwrap().merging, "{name}");
        let p = plan(&f.repo(), &act(r#"{"action":"abort"}"#));
        assert!(p.steps[0].contains(name.replace("pick", "cherry-pick").as_str()), "{:?}", p.steps);
        apply(&f.repo(), &act(r#"{"action":"abort"}"#));
        assert_eq!(state(f.dir()), before, "{name}");
    }
}

#[test]
fn nothing_to_abort() {
    let f = fixture("abort-nothing");
    let b = plan(&f.repo(), &act(r#"{"action":"abort"}"#)).blocked.unwrap();
    assert_eq!(b.reason, GitBlockReason::NothingToDo);
    assert!(f.repo().apply(&act(r#"{"action":"abort"}"#), &options(false)).is_err());
}

#[test]
fn refuses_while_the_app_has_unsaved_changes() {
    let f = conflicting("abort-unsaved");
    apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    let p = f.repo().plan(&act(r#"{"action":"abort"}"#), true).unwrap();
    assert_eq!(p.blocked.unwrap().reason, GitBlockReason::UnsavedChanges);
}

#[test]
fn aborts_a_rebase_started_on_the_command_line() {
    if !have_git() {
        return;
    }
    let f = conflicting("abort-rebase");
    apply(
        &f.repo(),
        &act(r#"{"action":"switch","target":"other"}"#),
    );
    let before = state(f.dir());
    // The rebase stops on the conflict with main's line five.
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(f.dir())
        .args(["rebase", "main"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", f.dir())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(f.dir().join(".git/rebase-merge").exists() || f.dir().join(".git/rebase-apply").exists());
    let p = plan(&f.repo(), &act(r#"{"action":"abort"}"#));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.steps[0].contains("rebase"), "{:?}", p.steps);
    let r = apply(&f.repo(), &act(r#"{"action":"abort"}"#));
    assert!(!f.dir().join(".git/rebase-merge").exists());
    assert!(!f.dir().join(".git/rebase-apply").exists());
    assert_eq!(state(f.dir()), before);
    // And Undo leaves no rebase behind.
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert!(!f.dir().join(".git/rebase-merge").exists());
}

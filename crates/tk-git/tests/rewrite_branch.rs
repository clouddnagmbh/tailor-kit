// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Authors

//! Rewrite (wave 15, strand D): a branch is pointed at a commit a history editor built with
//! `pick`, `write_tree` and `write_commit`. The plan counts the commits replaced and brought;
//! one backup ref and one Undo step cover it all; the checked-out branch's files follow;
//! published commits and a missing tip are refused.

mod common;
mod wizard;

use common::*;
use tk_git_types::actions::GitBlockReason;
use tk_git_types::plumbing::{GitCommitWrite, GitPick, GitSignature};
use wizard::*;

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

fn have_git() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok()
}

/// Branch `feature` (checked out) with three commits on top of `main`'s "init":
/// c1 changes the notes, c2 adds a YAML document, c3 changes the notes again. Returns their ids.
fn feature(name: &str) -> (Fixture, [String; 3]) {
    let f = fixture(name);
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"feature","start":"HEAD","switch":true}"#),
    );
    let c1 = f.commit("c1", &[("notes.txt", &NOTES.replacen("one", "ONE", 1))]);
    let c2 = f.commit(
        "c2",
        &[("users/new.yml", &http_doc("GET", "https://api.test/new"))],
    );
    let c3 = f.commit(
        "c3",
        &[(
            "notes.txt",
            &NOTES.replacen("one", "ONE", 1).replacen("ten", "TEN", 1),
        )],
    );
    (f, [c1, c2, c3])
}

/// One commit with `tree`'s content on top of `parent`, authored as `commit`.
fn rebuilt(f: &Fixture, tree: &str, parent: &str, message: &str, author_of: &str) -> String {
    let repo = f.repo();
    let info = repo.commit_info(author_of).unwrap();
    repo.write_commit(
        &GitCommitWrite {
            tree: tree.into(),
            parents: vec![parent.into()],
            message: message.into(),
            author: Some(info.author),
            committer: None,
        },
        None,
    )
    .unwrap()
}

fn rewrite(branch: &str, tip: &str) -> tk_git_types::actions::GitAction {
    act(&format!(
        r#"{{"action":"rewrite","branch":"{branch}","tip":"{tip}","summary":"Dropped c2"}}"#
    ))
}

#[test]
fn drops_a_commit_with_pick_and_rewrite_like_git_would() {
    let (f, [c1, c2, c3]) = feature("rewrite-drop");
    let repo = f.repo();
    // Replay c3 onto c1: c2 is gone.
    let picked = repo
        .pick(&GitPick {
            onto: c1.clone(),
            commit: c3.clone(),
            revert: false,
            mainline: None,
        })
        .unwrap();
    assert!(picked.clean);
    let new3 = rebuilt(&f, &picked.tree, &c1, "c3\n", &c3);
    let before = state(f.dir());
    let action = rewrite("feature", &new3);
    let p = plan(&f.repo(), &action);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.backup && p.undoable);
    assert!(
        p.steps.iter().any(|s| s.contains("Dropped c2")),
        "{:?}",
        p.steps
    );
    assert!(
        p.steps
            .iter()
            .any(|s| s.contains("Replaces 2 commits on feature with 1")),
        "{:?}",
        p.steps
    );
    assert_eq!(p.files, paths(&["/users/new.yml"]));
    assert_eq!(p.refs.len(), 1);
    assert_eq!(p.refs[0].before.as_deref(), Some(c3.as_str()));
    assert_eq!(p.refs[0].after.as_deref(), Some(new3.as_str()));
    assert_eq!(raw_state(f.dir()), {
        let _ = plan(&f.repo(), &action);
        raw_state(f.dir())
    });

    let r = apply(&f.repo(), &action);
    assert!(r.reload);
    assert_eq!(r.message, "Dropped c2.");
    let entry = r.undo.unwrap();
    assert_eq!(
        entry.backups.len(),
        1,
        "one backup for the whole rewrite: {:?}",
        entry.backups
    );
    assert_eq!(
        ref_id(f.dir(), &entry.backups[0]).as_deref(),
        Some(c3.as_str())
    );
    assert_eq!(f.repo().rev_parse("feature").unwrap(), new3);
    assert_eq!(
        f.repo().status().unwrap().branch.as_deref(),
        Some("feature")
    );
    assert!(!f.exists("users/new.yml"), "the working files follow");
    assert!(f.repo().status().unwrap().files.is_empty());
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(state(f.dir()), before);
    if have_git() {
        // `git rebase --onto c1 c2 feature` (drop c2) gives the same tree.
        let twin = tmp("rewrite-drop-twin");
        copy_dir(f.dir(), twin.path());
        let g = |args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(twin.path())
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("HOME", twin.path())
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        g(&["rebase", "--onto", &c1, &c2, "feature"]);
        assert_eq!(
            g(&["rev-parse", "HEAD^{tree}"]).trim(),
            f.repo().commit_info(&new3).unwrap().tree
        );
    }
}

#[test]
fn squashes_three_commits_into_one() {
    let (f, [_, _, c3]) = feature("rewrite-squash");
    let base = f.repo().rev_parse("main").unwrap();
    let info = f.repo().commit_info(&c3).unwrap();
    let squashed = rebuilt(&f, &info.tree, &base, "everything\n", &c3);
    let before = state(f.dir());
    let r = apply(&f.repo(), &rewrite("feature", &squashed));
    assert_eq!(
        f.read("notes.txt"),
        NOTES.replacen("one", "ONE", 1).replacen("ten", "TEN", 1)
    );
    assert!(f.exists("users/new.yml"), "same tree: same files");
    assert!(f.repo().status().unwrap().files.is_empty());
    assert_eq!(f.repo().commit_info("feature").unwrap().parents, vec![base]);
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(state(f.dir()), before);
    let p = plan(&f.repo(), &rewrite("feature", &squashed));
    assert!(
        p.steps
            .iter()
            .any(|s| s.contains("Replaces 3 commits on feature with 1")),
        "{:?}",
        p.steps
    );
}

#[test]
fn rewrites_a_branch_that_is_not_checked_out_without_touching_files() {
    let (f, [c1, _, c3]) = feature("rewrite-other");
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    let before = state(f.dir());
    let a = rewrite("feature", &c1);
    let p = plan(&f.repo(), &a);
    assert!(p.files.is_empty());
    assert!(
        p.steps
            .iter()
            .any(|s| s.contains("Replaces 2 commits on feature with 0")),
        "{:?}",
        p.steps
    );
    let r = apply(&f.repo(), &a);
    assert!(!r.reload);
    assert_eq!(f.repo().rev_parse("feature").unwrap(), c1);
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(f.repo().rev_parse("feature").unwrap(), c3);
    assert_eq!(state(f.dir()), before);
}

#[test]
fn refuses_to_replace_commits_that_are_on_the_remote() {
    let (f, [c1, _, c3]) = feature("rewrite-published");
    // c1 and c2 go to the server; c3 stays local.
    apply(
        &f.repo(),
        &act(r#"{"action":"publish","branch":"feature","remote":"origin"}"#),
    );
    let tip_before = f.repo().rev_parse("feature").unwrap();
    // c3 is published too now: refused.
    let b = plan(&f.repo(), &rewrite("feature", &c1)).blocked.unwrap();
    assert_eq!(b.reason, GitBlockReason::Published);
    assert!(b.message.contains("server"), "{}", b.message);
    let before = raw_state(f.dir());
    assert!(
        f.repo()
            .apply(&rewrite("feature", &c1), &options(false))
            .is_err()
    );
    assert_eq!(raw_state(f.dir()), before);
    assert_eq!(f.repo().rev_parse("feature").unwrap(), tip_before);

    // A new local commit on top may be replaced; the published ones stay as its parents.
    let c4 = f.commit("c4", &[("notes.txt", "four\n")]);
    let info = f.repo().commit_info(&c4).unwrap();
    let reword = rebuilt(&f, &info.tree, &c3, "c4 reworded\n", &c4);
    assert_eq!(reason(&f.repo(), &rewrite("feature", &reword)), None);
    apply(&f.repo(), &rewrite("feature", &reword));
    assert_eq!(
        f.repo().commit_info("feature").unwrap().message.trim(),
        "c4 reworded"
    );
}

#[test]
fn refuses_what_cannot_run() {
    let (f, [c1, _, c3]) = feature("rewrite-blocked");
    for (json, want) in [
        (
            r#"{"action":"rewrite","branch":"nope","tip":"HEAD","summary":""}"#.to_owned(),
            GitBlockReason::NotFound,
        ),
        (
            r#"{"action":"rewrite","branch":"feature","tip":"deadbeef","summary":""}"#.to_owned(),
            GitBlockReason::NotFound,
        ),
        (
            format!(r#"{{"action":"rewrite","branch":"feature","tip":"{c3}","summary":""}}"#),
            GitBlockReason::NothingToDo,
        ),
    ] {
        assert_eq!(reason(&f.repo(), &act(&json)), Some(want), "{json}");
    }
    let a = rewrite("feature", &c1);
    assert_eq!(
        f.repo().plan(&a, true).unwrap().blocked.unwrap().reason,
        GitBlockReason::UnsavedChanges
    );
    f.write("notes.txt", "local edit\n");
    assert_eq!(reason(&f.repo(), &a), Some(GitBlockReason::LocalChanges));
    // The branch that is not checked out does not care about the files.
    let (g, [d1, _, _]) = feature("rewrite-blocked-2");
    apply(&g.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    g.write("notes.txt", "local edit\n");
    assert_eq!(reason(&g.repo(), &rewrite("feature", &d1)), None);
}

#[test]
fn undo_is_refused_once_the_branch_moved_on() {
    let (f, [c1, _, _]) = feature("rewrite-stale");
    let r = apply(&f.repo(), &rewrite("feature", &c1));
    f.commit("later", &[("notes.txt", "later\n")]);
    let err = f.repo().undo(&r.undo.unwrap().id, false).unwrap_err();
    assert!(err.to_string().contains("Cannot undo"), "{err}");
}

#[test]
fn keeps_untracked_files_and_the_author_of_rebuilt_commits() {
    let (f, [c1, _, c3]) = feature("rewrite-untracked");
    f.write("loose.txt", "untracked\n");
    let info = f.repo().commit_info(&c3).unwrap();
    let a = rebuilt(
        &f,
        &f.repo().commit_info(&c1).unwrap().tree,
        &f.repo().rev_parse("main").unwrap(),
        "squash\n",
        &c3,
    );
    apply(&f.repo(), &rewrite("feature", &a));
    assert_eq!(f.read("loose.txt"), "untracked\n");
    let now = f.repo().commit_info("feature").unwrap();
    let sig: &GitSignature = &now.author;
    assert_eq!(sig, &info.author);
    assert_ne!(now.committer.time, 0);
}

#[test]
fn fast_forward_to_a_descendant_is_accepted_even_when_published() {
    let (f, [c1, _, c3]) = feature("rewrite-ff");
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    // feature is rewound to c1, then moved forward to c3 again: nothing is dropped.
    apply(&f.repo(), &rewrite("feature", &c1));
    apply(
        &f.repo(),
        &act(r#"{"action":"publish","branch":"feature","remote":"origin"}"#),
    );
    let p = plan(&f.repo(), &rewrite("feature", &c3));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    apply(&f.repo(), &rewrite("feature", &c3));
    assert_eq!(f.repo().rev_parse("feature").unwrap(), c3);
}

#[test]
fn cherry_pick_takes_a_commit_that_only_exists_as_an_object() {
    let (f, [c1, _, c3]) = feature("pick-loose");
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    let tree = f.repo().commit_info(&c3).unwrap().tree;
    let loose = f
        .repo()
        .write_commit(
            &GitCommitWrite {
                tree,
                parents: vec![c1.clone()],
                message: "loose\n".into(),
                author: None,
                committer: None,
            },
            None,
        )
        .unwrap();
    let a = act(&format!(r#"{{"action":"cherry-pick","commit":"{loose}"}}"#));
    assert_eq!(reason(&f.repo(), &a), None);
    apply(&f.repo(), &a);
    assert!(
        f.repo()
            .commit_info("HEAD")
            .unwrap()
            .message
            .contains("loose")
    );
}

#[test]
fn file_diff_without_from_on_a_root_commit_shows_the_whole_file() {
    let f = fixture("diff-root");
    let root = f.repo().rev_parse("HEAD").unwrap();
    let d = f.repo().file_diff(None, Some(&root), "/notes.txt").unwrap();
    assert!(!d.binary);
    assert!(!d.hunks.is_empty());
}

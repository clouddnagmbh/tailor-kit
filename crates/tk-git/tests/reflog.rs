// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Authors

//! Wave 15, strand C: the reflog of `HEAD` and of branches shows what Git did to them, also
//! the lost work. Compared with `git reflog` when Git is installed.

mod common;

use std::path::Path;
use std::process::Command;

use common::*;

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn git_reflog(dir: &Path, name: &str) -> Vec<(String, String)> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["reflog", "show", "--format=%H %gs", name, "--"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| {
            let (id, msg) = l.split_once(' ').unwrap();
            (id.to_owned(), msg.to_owned())
        })
        .collect()
}

#[test]
fn the_reflog_lists_every_move_newest_first() {
    let t = tmp("reflog");
    let dir = t.path();
    tk_git::init(dir, &opts()).unwrap();
    set_user(dir, "Ada");
    let repo = reopen(dir);
    write(dir, "a.txt", "one\n");
    let c1 = repo.commit("first", &paths(&["a.txt"]), None).unwrap();
    write(dir, "a.txt", "two\n");
    let c2 = repo.commit("second", &paths(&["a.txt"]), None).unwrap();
    write(dir, "a.txt", "three\n");
    let c3 = repo.commit("third", &paths(&["a.txt"]), None).unwrap();

    let log = repo.reflog("HEAD", 50).unwrap();
    assert_eq!(log.len(), 3);
    assert_eq!(log.iter().map(|e| e.index).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(log[0].new, c3.oid);
    assert_eq!(log[0].old.as_deref(), Some(c2.oid.as_str()));
    assert_eq!(log[2].new, c1.oid);
    assert_eq!(
        log[2].old, None,
        "before the first commit the ref did not exist"
    );
    assert_eq!(log[0].name, "HEAD");
    assert!(log[0].message.contains("third"), "{}", log[0].message);
    assert_eq!(log[0].commit.as_ref().unwrap().summary, "third");
    assert!(log[0].time > 1_700_000_000);

    // A branch by its short or full name; a limit; a ref without a log.
    assert_eq!(repo.reflog("main", 50).unwrap().len(), 3);
    assert_eq!(repo.reflog("refs/heads/main", 2).unwrap().len(), 2);
    assert!(repo.reflog("nobranch", 10).unwrap().is_empty());
    assert!(repo.reflog("", 10).is_err());

    if have_git() {
        let want = git_reflog(dir, "HEAD");
        let got: Vec<(String, String)> = log
            .iter()
            .map(|e| (e.new.clone(), e.message.clone()))
            .collect();
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            assert_eq!(g.0, w.0);
        }
    }
}

#[test]
fn a_reset_branch_is_still_in_the_reflog() {
    let t = tmp("reflog-lost");
    let dir = t.path();
    tk_git::init(dir, &opts()).unwrap();
    set_user(dir, "Ada");
    let repo = reopen(dir);
    write(dir, "a.txt", "one\n");
    let c1 = repo.commit("first", &paths(&["a.txt"]), None).unwrap();
    write(dir, "a.txt", "two\n");
    let lost = repo
        .commit("work that gets lost", &paths(&["a.txt"]), None)
        .unwrap();
    // Move the branch back, as `git reset --hard HEAD~1` does.
    let g = gix::open_opts(dir, gix::open::Options::isolated()).unwrap();
    g.reference(
        "refs/heads/main",
        gix::ObjectId::from_hex(c1.oid.as_bytes()).unwrap(),
        gix::refs::transaction::PreviousValue::Any,
        "reset: moving to HEAD~1",
    )
    .unwrap();

    let log = reopen(dir).reflog("main", 50).unwrap();
    assert_eq!(log[0].new, c1.oid);
    assert_eq!(log[0].old.as_deref(), Some(lost.oid.as_str()));
    assert!(log[0].message.contains("reset"), "{}", log[0].message);
    // The commit the reset left behind is named by an older entry: "lost work".
    assert!(
        log.iter()
            .any(|e| e.new == lost.oid
                && e.commit.as_ref().unwrap().summary == "work that gets lost")
    );
}

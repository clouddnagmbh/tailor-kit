// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Authors

//! Choosing lines (wave 15, strand F): staging, unstaging and discarding chosen lines gives
//! the index and working tree that `git apply --cached`, `git apply --cached -R` and
//! `git apply -R` give for the same lines (the patch is built in the test the way
//! `git add -p` builds it from an edited hunk), and `git add -p` itself for whole hunks.

mod common;
mod wizard;

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use common::*;
use tk_git_types::index::{GitHunkSelection, GitIndexDiff, GitPatch, GitPatchSide};
use tk_git_types::{DiffLineKind, GitDiff};
use wizard::*;

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// The rest of the work tree and index, copied, so `git` can do the same on its own copy.
fn copy_of(f: &Fixture, name: &str) -> Tmp {
    let to = tmp(name);
    let status = Command::new("cp")
        .arg("-R")
        .arg(format!("{}/.", f.dir().display()))
        .arg(to.path())
        .status()
        .unwrap();
    assert!(status.success());
    to
}

/// A unified diff of `diff` with only the chosen lines (hunk, lines; `None`: all), as
/// `git add -p` writes an edited hunk: unchosen removals become context, unchosen additions
/// are dropped.
fn patch_text(diff: &GitDiff, choose: &[(usize, Option<Vec<usize>>)], reverse: bool) -> String {
    let mut choose = choose.to_vec();
    choose.sort_by_key(|(h, _)| *h);
    let mut out = format!("--- a/{0}\n+++ b/{0}\n", diff.path.trim_start_matches('/'));
    for (h, lines) in &choose {
        let hunk = &diff.hunks[*h];
        let chosen = |i: usize| lines.as_ref().is_none_or(|l| l.contains(&i));
        let mut body = String::new();
        let (mut old, mut new) = (0, 0);
        for (i, line) in hunk.lines.iter().enumerate() {
            match line.kind {
                DiffLineKind::Context => {
                    body.push_str(&format!(" {}\n", line.text));
                    old += 1;
                    new += 1;
                }
                DiffLineKind::Removed if chosen(i) => {
                    body.push_str(&format!("-{}\n", line.text));
                    old += 1;
                }
                DiffLineKind::Removed if reverse => {}
                DiffLineKind::Removed => {
                    body.push_str(&format!(" {}\n", line.text));
                    old += 1;
                    new += 1;
                }
                DiffLineKind::Added if chosen(i) => {
                    body.push_str(&format!("+{}\n", line.text));
                    new += 1;
                }
                DiffLineKind::Added if reverse => {
                    body.push_str(&format!(" {}\n", line.text));
                    old += 1;
                    new += 1;
                }
                DiffLineKind::Added => {}
            }
        }
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n{body}",
            hunk.old_start, old, hunk.new_start, new
        ));
    }
    out
}

fn git_apply(dir: &Path, patch: &str, args: &[&str]) {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("apply")
        .args(args)
        .arg("--unidiff-zero")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(patch.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "git apply {args:?}: {}\n{patch}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn patch_of(d: &GitIndexDiff, hunks: Vec<(u32, Option<Vec<u32>>)>) -> GitPatch {
    GitPatch {
        path: "/long.txt".into(),
        side: d.side,
        old: d.old.clone(),
        new: d.new.clone(),
        hunks: hunks
            .into_iter()
            .map(|(hunk, lines)| GitHunkSelection { hunk, lines })
            .collect(),
    }
}

fn staged_ids(dir: &Path) -> String {
    git(dir, &["ls-files", "-s"]).unwrap()
}

fn long() -> String {
    (1..=30).map(|n| format!("l{n:02}\n")).collect()
}

/// `long()` with line 1 replaced, a line added after line 3 and line 30 replaced: two hunks.
fn edited() -> String {
    long()
        .replace("l01\n", "L01\n")
        .replace("l03\n", "l03\nl03b\n")
        .replace("l30\n", "L30\n")
}

/// A fixture with `long.txt` committed.
fn long_fixture(name: &str) -> Fixture {
    let f = fixture(name);
    f.commit("long", &[("long.txt", &long())]);
    f
}

#[test]
fn staging_chosen_lines_equals_git_apply_cached() {
    if !have_git() {
        return;
    }
    let f = long_fixture("patch-stage");
    f.write("long.txt", &edited());
    let repo = f.repo();
    let d = repo
        .index_diff("/long.txt", GitPatchSide::Unstaged)
        .unwrap();
    assert_eq!(d.diff.hunks.len(), 2, "{:?}", d.diff);
    // First hunk: lines are -one +ONE, two, three, +THREE-B, four... pick only "+ONE" and
    // "+THREE-B"; leave "-one" (so "one" stays and "ONE" is added next to it).
    let h0 = &d.diff.hunks[0];
    let added: Vec<usize> = h0
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == DiffLineKind::Added)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(added.len(), 2);
    let copy = copy_of(&f, "patch-stage-git");
    git_apply(
        copy.path(),
        &patch_text(&d.diff, &[(0, Some(added.clone()))], false),
        &["--cached"],
    );
    repo.stage_patch(&patch_of(
        &d,
        vec![(0, Some(added.iter().map(|i| *i as u32).collect()))],
    ))
    .unwrap();
    assert_eq!(staged_ids(f.dir()), staged_ids(copy.path()));
    // The working file is untouched, the index differs from HEAD.
    assert_eq!(f.read("long.txt"), edited());
    let staged = repo.index_diff("/long.txt", GitPatchSide::Staged).unwrap();
    assert!(!staged.diff.hunks.is_empty());
    // `git diff --cached` agrees about what is staged.
    assert_eq!(
        git(f.dir(), &["diff", "--cached", "--numstat"]),
        git(copy.path(), &["diff", "--cached", "--numstat"])
    );
}

#[test]
fn a_whole_hunk_equals_git_add_p() {
    if !have_git() {
        return;
    }
    let f = long_fixture("patch-hunk");
    f.write("long.txt", &edited());
    let repo = f.repo();
    let d = repo
        .index_diff("/long.txt", GitPatchSide::Unstaged)
        .unwrap();
    // `git add -p`: stage the second hunk only (answer n, y).
    let copy = copy_of(&f, "patch-hunk-git");
    let mut child = Command::new("git")
        .arg("-C")
        .arg(copy.path())
        .args(["add", "-p", "long.txt"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\ny\n").unwrap();
    assert!(child.wait().unwrap().success());
    repo.stage_patch(&patch_of(&d, vec![(1, None)])).unwrap();
    assert_eq!(staged_ids(f.dir()), staged_ids(copy.path()));
}

#[test]
fn unstaging_chosen_lines_equals_git_apply_cached_reverse() {
    if !have_git() {
        return;
    }
    let f = long_fixture("patch-unstage");
    f.write("long.txt", &edited());
    git(f.dir(), &["add", "long.txt"]);
    let repo = f.repo();
    let d = repo.index_diff("/long.txt", GitPatchSide::Staged).unwrap();
    assert_eq!(d.diff.hunks.len(), 2);
    let copy = copy_of(&f, "patch-unstage-git");
    let pick = vec![(1usize, None), (0usize, Some(vec![1usize]))];
    // Reverse of the chosen part of the staged diff.
    git_apply(
        copy.path(),
        &patch_text(&d.diff, &pick, true),
        &["--cached", "-R"],
    );
    repo.unstage_patch(&patch_of(&d, vec![(1, None), (0, Some(vec![1]))]))
        .unwrap();
    assert_eq!(staged_ids(f.dir()), staged_ids(copy.path()));
    // Unstaging everything is exactly `HEAD`.
    let d = repo.index_diff("/long.txt", GitPatchSide::Staged).unwrap();
    let all: Vec<_> = (0..d.diff.hunks.len() as u32).map(|h| (h, None)).collect();
    repo.unstage_patch(&patch_of(&d, all)).unwrap();
    assert_eq!(git(f.dir(), &["diff", "--cached"]).unwrap(), "");
    assert_eq!(f.read("long.txt"), edited());
}

#[test]
fn discarding_chosen_lines_equals_git_apply_reverse() {
    if !have_git() {
        return;
    }
    let f = long_fixture("patch-discard");
    f.write("long.txt", &edited());
    let repo = f.repo();
    let d = repo
        .index_diff("/long.txt", GitPatchSide::Unstaged)
        .unwrap();
    let copy = copy_of(&f, "patch-discard-git");
    let pick = vec![(1usize, None), (0usize, Some(vec![4usize]))];
    git_apply(copy.path(), &patch_text(&d.diff, &pick, true), &["-R"]);
    repo.discard_patch(&patch_of(&d, vec![(1, None), (0, Some(vec![4]))]))
        .unwrap();
    assert_eq!(f.read("long.txt"), read(copy.path(), "long.txt"));
    // The index did not move; the file as it was is kept under a backup ref.
    assert_eq!(git(f.dir(), &["diff", "--cached"]).unwrap(), "");
    let refs = git(f.dir(), &["for-each-ref", "refs/tailor/backup"]).unwrap();
    let line = refs.lines().find(|l| l.contains("/discard")).expect(&refs);
    let id = line.split_whitespace().next().unwrap();
    assert_eq!(
        git(f.dir(), &["show", &format!("{id}:long.txt")]).unwrap(),
        edited()
    );
}

#[test]
fn a_stale_selection_is_refused() {
    let f = long_fixture("patch-stale");
    f.write("long.txt", &edited());
    let repo = f.repo();
    let d = repo
        .index_diff("/long.txt", GitPatchSide::Unstaged)
        .unwrap();
    // The file changes after the diff was read.
    f.write("long.txt", &format!("{}eleven\n", edited()));
    let err = repo
        .stage_patch(&patch_of(&d, vec![(0, None)]))
        .unwrap_err();
    assert!(err.to_string().contains("changed since"), "{err}");
    let err = repo
        .discard_patch(&patch_of(&d, vec![(0, None)]))
        .unwrap_err();
    assert!(err.to_string().contains("changed since"), "{err}");
    // Nothing happened.
    assert_eq!(f.read("long.txt"), format!("{}eleven\n", edited()));
    assert_eq!(git_or_empty(f.dir()), "");
    // Wrong side and unknown lines are refused too.
    let fresh = repo
        .index_diff("/long.txt", GitPatchSide::Unstaged)
        .unwrap();
    let mut wrong = patch_of(&fresh, vec![(0, None)]);
    wrong.side = GitPatchSide::Staged;
    assert!(repo.stage_patch(&wrong).is_err());
    assert!(
        repo.stage_patch(&patch_of(&fresh, vec![(7, None)]))
            .is_err()
    );
    assert!(
        repo.stage_patch(&patch_of(&fresh, vec![(0, Some(vec![99]))]))
            .is_err()
    );
}

fn git_or_empty(dir: &Path) -> String {
    git(dir, &["diff", "--cached"]).unwrap_or_default()
}

#[test]
fn new_and_deleted_files_by_lines() {
    if !have_git() {
        return;
    }
    let f = long_fixture("patch-new");
    // A new file: stage its first two lines.
    f.write("new.txt", "a\nb\nc\n");
    let repo = f.repo();
    let d = repo.index_diff("/new.txt", GitPatchSide::Unstaged).unwrap();
    assert_eq!(d.old, "");
    let mut req = patch_of(&d, vec![(0, Some(vec![0, 1]))]);
    req.path = "/new.txt".into();
    repo.stage_patch(&req).unwrap();
    assert_eq!(git(f.dir(), &["show", ":new.txt"]).unwrap(), "a\nb\n");
    // Unstage all of it: the file leaves the index and stays untracked.
    let d = repo.index_diff("/new.txt", GitPatchSide::Staged).unwrap();
    let mut req = patch_of(&d, vec![(0, None)]);
    req.path = "/new.txt".into();
    repo.unstage_patch(&req).unwrap();
    assert!(git(f.dir(), &["ls-files", "new.txt"]).unwrap().is_empty());
    assert_eq!(f.read("new.txt"), "a\nb\nc\n");

    // A deleted file: staging all its lines stages the deletion.
    std::fs::remove_file(f.dir().join("long.txt")).unwrap();
    let d = repo
        .index_diff("/long.txt", GitPatchSide::Unstaged)
        .unwrap();
    assert_eq!(d.new, "");
    assert_eq!(d.diff.hunks.len(), 1);
    repo.stage_patch(&patch_of(&d, vec![(0, None)])).unwrap();
    assert_eq!(git(f.dir(), &["ls-files", "long.txt"]).unwrap(), "");
    assert!(
        git(f.dir(), &["diff", "--cached", "--stat"])
            .unwrap()
            .contains("long.txt")
    );
}

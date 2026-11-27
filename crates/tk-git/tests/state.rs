// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The state API (`tk_git::state`): capturing the working files, the index, its conflict
//! stages and a merge in progress changes nothing but the object store; setting a captured
//! state back gives what real `git` saw then (`ls-files -s`, `status`, `for-each-ref`,
//! `symbolic-ref`, the bytes of every file); stale expectations are refused with nothing
//! changed.

mod common;

use std::path::Path;
use std::process::Command;

use common::*;
use tk_git::Repo;
use tk_git::state::{
    CaptureOptions, RecordEntry, RefState, StateExpected, StateTarget, WorkingState,
};

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// Real `git` with a test identity and no user configuration; panics on failure.
fn g(dir: &Path, args: &[&str]) -> String {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap()
}

fn repo(name: &str) -> Tmp {
    let t = tmp(name);
    g(t.path(), &["init", "-q", "-b", "main"]);
    g(t.path(), &["config", "core.autocrlf", "false"]);
    write(t.path(), ".gitignore", "*.log\n");
    write(t.path(), "a.txt", "a\n");
    write(t.path(), "dir/b.txt", "b\n");
    write(t.path(), "gone.txt", "gone\n");
    write(t.path(), "run.sh", "#!/bin/sh\n");
    g(t.path(), &["add", "-A"]);
    g(t.path(), &["commit", "-qm", "init"]);
    t
}

fn open(dir: &Path) -> Repo {
    Repo::open_repository(dir, &opts()).unwrap().unwrap()
}

/// Everything `git` shows of the state: refs, HEAD, index, status, merge files, file bytes
/// (with the executable bit and symbolic links).
fn seen(dir: &Path) -> String {
    let git_dir = dir.join(".git");
    let merge: Vec<String> = tk_git::state::MERGE_FILES
        .iter()
        .map(|n| format!("{n}: {:?}", std::fs::read(git_dir.join(n)).ok()))
        .collect();
    format!(
        "refs:\n{}\nHEAD: {}\nindex:\n{}\nstatus:\n{}\nmerge: {merge:?}\nfiles:\n{}",
        for_each_ref(dir),
        std::fs::read_to_string(git_dir.join("HEAD")).unwrap(),
        g(dir, &["ls-files", "-s"]),
        g(dir, &["status", "--porcelain=v2", "--untracked-files=all"]),
        disk(dir)
    )
}

/// `git for-each-ref` without the product's own refs.
fn for_each_ref(dir: &Path) -> String {
    g(dir, &["for-each-ref", "--format=%(objectname) %(refname)"])
        .lines()
        .filter(|l| !l.contains(" refs/tailor/"))
        .map(|l| format!("{l}\n"))
        .collect()
}

fn disk(dir: &Path) -> String {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            if e.file_name() == ".git" {
                continue;
            }
            let p = e.path();
            let md = std::fs::symlink_metadata(&p).unwrap();
            let rel = p.strip_prefix(dir).unwrap().to_string_lossy().into_owned();
            if md.is_symlink() {
                out.push(format!("{rel} -> {:?}", std::fs::read_link(&p).unwrap()));
            } else if md.is_dir() {
                stack.push(p);
            } else {
                out.push(format!(
                    "{rel} {} {:?}",
                    exec(&md),
                    std::fs::read(&p).unwrap()
                ));
            }
        }
    }
    out.sort();
    out.join("\n")
}

#[cfg(unix)]
fn exec(md: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    md.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn exec(_md: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn chmod_x(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Copies the repository folder (with `.git`) for comparisons that let `git` write.
fn copy(from: &Path, name: &str) -> Tmp {
    let t = tmp(name);
    let status = Command::new("cp")
        .arg("-R")
        .arg(format!("{}/.", from.display()))
        .arg(t.path())
        .status()
        .unwrap();
    assert!(status.success());
    t
}

fn edit(dir: &Path) {
    write(dir, "a.txt", "a changed\r\nwith crlf\r\n");
    std::fs::remove_file(dir.join("gone.txt")).unwrap();
    write(dir, "new/deep/c.txt", "untracked\n");
    write(dir, "debug.log", "ignored\n");
    write(dir, "staged.txt", "staged\n");
    g(dir, &["add", "staged.txt"]);
    write(dir, "staged.txt", "staged, then edited\n");
    #[cfg(unix)]
    {
        chmod_x(&dir.join("dir/b.txt"));
        std::os::unix::fs::symlink("a.txt", dir.join("link")).unwrap();
    }
}

fn capture(dir: &Path) -> WorkingState {
    open(dir)
        .capture_working_state(&CaptureOptions::default())
        .unwrap()
}

#[test]
fn capture_equals_git_add_all_and_changes_nothing() {
    if !have_git() {
        return;
    }
    let t = repo("state-capture");
    let dir = t.path();
    edit(dir);
    let index_before = std::fs::read(dir.join(".git/index")).unwrap();
    let seen_before = seen(dir);
    let state = capture(dir);
    assert_eq!(std::fs::read(dir.join(".git/index")).unwrap(), index_before);
    assert_eq!(seen(dir), seen_before);

    // The index tree is what `git write-tree` writes, the files tree what `git add -A` stages.
    let c = copy(dir, "state-capture-copy");
    assert_eq!(g(c.path(), &["write-tree"]).trim(), state.index);
    g(c.path(), &["add", "-A"]);
    assert_eq!(g(c.path(), &["write-tree"]).trim(), state.files);
    let listed = g(dir, &["ls-tree", "-r", "--name-only", &state.files]);
    assert!(!listed.contains("debug.log"), "{listed}");
    assert!(listed.contains("new/deep/c.txt"), "{listed}");
    assert!(state.conflicts.is_none() && state.merge.is_none());
    assert!(state.changed.contains(&"/gone.txt".to_owned()));
    assert!(state.changed.contains(&"/new/deep/c.txt".to_owned()));
}

#[test]
fn large_files_stay_out() {
    if !have_git() {
        return;
    }
    let t = repo("state-large");
    let dir = t.path();
    write(dir, "big.bin", &"x".repeat(2000));
    write(dir, "a.txt", &"y".repeat(2000));
    let state = open(dir)
        .capture_working_state(&CaptureOptions {
            max_file_bytes: Some(1000),
        })
        .unwrap();
    assert_eq!(
        state.skipped,
        vec!["/a.txt".to_owned(), "/big.bin".to_owned()]
    );
    let r = open(dir);
    assert_eq!(
        r.record_blob(&state.files, "a.txt").unwrap().unwrap(),
        b"a\n"
    );
    assert!(r.record_entry(&state.files, "big.bin").unwrap().is_none());
}

#[test]
fn ref_state_text_is_for_each_ref() {
    if !have_git() {
        return;
    }
    let t = repo("state-refs");
    let dir = t.path();
    g(dir, &["branch", "feature/x"]);
    g(dir, &["tag", "-a", "v1", "-m", "one"]);
    g(dir, &["tag", "light"]);
    g(dir, &["update-ref", "refs/tailor/backup/1/main", "HEAD"]);
    let state = open(dir).ref_state().unwrap();
    let text = state.to_text();
    let expected = format!("ref: refs/heads/main HEAD\n{}", for_each_ref(dir));
    assert_eq!(text, expected);
    assert_eq!(RefState::parse(&text).unwrap(), state);
    assert_eq!(state.head_branch(), Some("refs/heads/main"));
    assert_eq!(
        state.head_id().unwrap(),
        g(dir, &["rev-parse", "HEAD"]).trim()
    );
}

/// Captures the whole state into one commit like a product's snapshot would.
fn record(dir: &Path) -> (WorkingState, RefState) {
    (capture(dir), open(dir).ref_state().unwrap())
}

fn target(w: &WorkingState, r: &RefState) -> StateTarget {
    StateTarget {
        refs: Some(r.clone()),
        files: w.files.clone(),
        index: w.index.clone(),
        conflicts: w.conflicts.clone(),
        merge: w.merge.clone(),
        keep: Vec::new(),
    }
}

#[test]
fn setting_a_captured_state_gives_what_git_saw_then() {
    if !have_git() {
        return;
    }
    let t = repo("state-roundtrip");
    let dir = t.path();
    edit(dir);
    let then = seen(dir);
    let (w1, r1) = record(dir);

    // Everything moves on: commits, a new branch, a deleted file, a branch switch, a tag.
    g(dir, &["add", "-A"]);
    g(dir, &["commit", "-qm", "second"]);
    g(dir, &["switch", "-qc", "other"]);
    write(dir, "other.txt", "other\n");
    g(dir, &["add", "other.txt"]);
    g(dir, &["commit", "-qm", "third"]);
    g(dir, &["tag", "t1"]);
    g(dir, &["pack-refs", "--all"]);
    write(dir, "late.txt", "late, untracked\n");
    let (w2, r2) = record(dir);

    let preview = open(dir).state_preview(&target(&w1, &r1)).unwrap();
    assert!(
        preview
            .refs
            .iter()
            .any(|m| m.name == "refs/heads/other" && m.after.is_none())
    );
    assert!(preview.refs.iter().any(|m| m.name == "refs/tags/t1"));
    assert!(preview.files.iter().any(|f| f.path == "/late.txt"));
    assert!(preview.index);
    let before_preview = seen(dir);
    assert_eq!(seen(dir), before_preview, "a preview changes nothing");

    let change = open(dir)
        .set_state(
            &target(&w1, &r1),
            &StateExpected {
                refs: Some(r2.clone()),
                files: w2.files.clone(),
            },
        )
        .unwrap();
    assert_eq!(change.refs, preview.refs);
    assert_eq!(seen(dir), then);

    // And forward again.
    open(dir)
        .set_state(
            &target(&w2, &r2),
            &StateExpected {
                refs: Some(r1.clone()),
                files: w1.files.clone(),
            },
        )
        .unwrap();
    let (w3, r3) = record(dir);
    assert_eq!((w3.files, w3.index, r3), (w2.files, w2.index, r2));
    assert_eq!(g(dir, &["fsck", "--no-dangling"]), "");
}

#[test]
fn conflicts_and_a_merge_in_progress_round_trip() {
    if !have_git() {
        return;
    }
    let t = repo("state-conflicts");
    let dir = t.path();
    g(dir, &["switch", "-qc", "side"]);
    write(dir, "a.txt", "side\n");
    g(dir, &["commit", "-qam", "side"]);
    g(dir, &["switch", "-q", "main"]);
    write(dir, "a.txt", "main\n");
    g(dir, &["commit", "-qam", "main"]);
    let out = run(dir, &["merge", "side"]);
    assert!(!out.status.success(), "the merge conflicts");
    let then = seen(dir);
    assert!(then.contains("MERGE_HEAD: Some"));
    let (w1, r1) = record(dir);
    assert!(w1.conflicts.is_some() && w1.merge.is_some());
    let conflicts = w1.conflicts.clone().unwrap();
    let r = open(dir);
    assert!(r.record_entry(&conflicts, "1/a.txt").unwrap().is_some());
    assert!(r.record_entry(&conflicts, "3/a.txt").unwrap().is_some());

    g(dir, &["merge", "--abort"]);
    let (w2, r2) = record(dir);
    assert!(w2.merge.is_none());
    open(dir)
        .set_state(
            &target(&w1, &r1),
            &StateExpected {
                refs: Some(r2),
                files: w2.files,
            },
        )
        .unwrap();
    assert_eq!(seen(dir), then);
    // Git can conclude the merge it sees.
    write(dir, "a.txt", "both\n");
    g(dir, &["commit", "-qam", "merged"]);
    assert_eq!(
        g(dir, &["rev-list", "--count", "--merges", "HEAD"]).trim(),
        "1"
    );
}

#[test]
fn stale_expectations_are_refused_and_change_nothing() {
    if !have_git() {
        return;
    }
    let t = repo("state-stale");
    let dir = t.path();
    let (w1, r1) = record(dir);
    write(dir, "a.txt", "edit 1\n");
    let (w2, r2) = record(dir);

    // A ref moved since the caller looked.
    g(dir, &["commit", "-qam", "from outside"]);
    let now = seen(dir);
    let err = open(dir)
        .set_state(
            &target(&w1, &r1),
            &StateExpected {
                refs: Some(r2.clone()),
                files: w2.files.clone(),
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("changed since"), "{err}");
    assert_eq!(seen(dir), now);

    // A file to change was edited since.
    g(dir, &["reset", "-q", "--soft", "HEAD~1"]);
    write(dir, "a.txt", "edit 2\n");
    let now = seen(dir);
    let err = open(dir)
        .set_state(
            &target(&w1, &r1),
            &StateExpected {
                refs: Some(r2),
                files: w2.files,
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("a.txt"), "{err}");
    assert_eq!(seen(dir), now);
}

#[test]
fn files_only_and_kept_paths() {
    if !have_git() {
        return;
    }
    let t = repo("state-files-only");
    let dir = t.path();
    write(dir, "a.txt", "parked\n");
    write(dir, "big.bin", "big now\n");
    let w1 = capture(dir);
    g(dir, &["checkout", "--", "a.txt"]);
    write(dir, "big.bin", "big later\n");
    let w2 = capture(dir);
    let refs_before = g(dir, &["for-each-ref"]);
    let mut t1 = target(&w1, &open(dir).ref_state().unwrap());
    t1.refs = None;
    t1.keep = vec!["/big.bin".into()];
    open(dir)
        .set_state(
            &t1,
            &StateExpected {
                refs: None,
                files: w2.files,
            },
        )
        .unwrap();
    assert_eq!(read(dir, "a.txt"), "parked\n");
    assert_eq!(read(dir, "big.bin"), "big later\n", "kept as it is");
    assert_eq!(g(dir, &["for-each-ref"]), refs_before);
}

#[test]
fn own_refs_and_records() {
    if !have_git() {
        return;
    }
    let t = repo("state-own");
    let dir = t.path();
    let r = open(dir);
    let tree = r
        .write_record_tree(&[RecordEntry::Blob {
            name: "note.json".into(),
            bytes: b"{}".to_vec(),
        }])
        .unwrap();
    let head = g(dir, &["rev-parse", "HEAD"]).trim().to_owned();
    let c1 = r.write_record_commit(&tree, &[head], "one").unwrap();
    let c2 = r
        .write_record_commit(&tree, std::slice::from_ref(&c1), "two")
        .unwrap();
    assert!(
        r.set_own_ref("refs/heads/x", Some(&c1), None).is_err(),
        "not ours"
    );
    r.set_own_ref("refs/tailor/log", Some(&c1), None).unwrap();
    assert!(
        r.set_own_ref("refs/tailor/log", Some(&c2), None).is_err(),
        "exists"
    );
    assert!(
        r.set_own_ref("refs/tailor/log", Some(&c2), Some(&c2))
            .is_err(),
        "not the expected value"
    );
    r.set_own_ref("refs/tailor/log", Some(&c2), Some(&c1))
        .unwrap();
    assert_eq!(
        r.own_refs("").unwrap(),
        vec![("refs/tailor/log".to_owned(), c2.clone())]
    );
    assert_eq!(
        r.record_blob(&c2, "note.json").unwrap().unwrap(),
        b"{}".to_vec()
    );
    assert!(!dir.join(".git/logs/refs/tailor/log").exists(), "no reflog");
    r.set_own_ref("refs/tailor/log", None, Some(&c2)).unwrap();
    assert!(r.own_refs("").unwrap().is_empty());
    assert_eq!(
        g(dir, &["show", "-s", "--format=%an", &c2]).trim(),
        "Tailor"
    );
}

#[test]
fn linked_worktrees_have_their_own_dir() {
    if !have_git() {
        return;
    }
    let t = repo("state-worktree");
    let dir = t.path();
    let wt = dir
        .join("..")
        .join(format!("{}-wt", dir.file_name().unwrap().to_string_lossy()));
    g(
        dir,
        &[
            "worktree",
            "add",
            "-q",
            wt.to_str().unwrap(),
            "-b",
            "review",
        ],
    );
    let wt = tk_git::canonical(&wt).unwrap();
    let main = open(dir);
    let linked = open(&wt);
    assert_eq!(main.worktree_id(), None);
    assert_eq!(
        linked.worktree_id().as_deref(),
        Some(wt.file_name().unwrap().to_str().unwrap())
    );
    assert_eq!(main.own_dir(), dir.join(".git/tailor"));
    assert_eq!(linked.own_common_dir(), dir.join(".git/tailor"));
    assert!(linked.own_dir().starts_with(dir.join(".git/worktrees")));
    assert_eq!(
        linked.ref_state().unwrap().head_branch(),
        Some("refs/heads/review")
    );
    // The linked tree's capture sees its own index.
    write(&wt, "only-here.txt", "x\n");
    let w = capture(&wt);
    assert!(w.changed.contains(&"/only-here.txt".to_owned()));
    assert!(capture(dir).changed.is_empty());
    std::fs::remove_dir_all(&wt).unwrap();
}

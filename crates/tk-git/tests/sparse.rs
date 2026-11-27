// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Skip-worktree entries (a cone-mode sparse checkout made by real `git`): status, the working
//! copy, staging and committing treat them as unchanged; capturing and setting a state, a
//! switch and an undo keep the flag; `Repo::set_skip_worktree` gives what
//! `git sparse-checkout set --cone` gives; registered submodules can be staged and committed.
//! Real `git` is the witness only.

mod common;

use std::path::Path;
use std::process::Command;

use common::*;
use tk_git::state::{CaptureOptions, StateExpected, StateTarget};
use tk_git::{ActionOptions, NoCredentials, Repo};
use tk_git_types::GitResult;
use tk_git_types::actions::GitAction;
use tk_git_types::filter::{ContentFilter, FilterFile};

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

fn g(dir: &Path, args: &[&str]) -> String {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn open(dir: &Path) -> Repo {
    Repo::open_repository(dir, &opts()).unwrap().unwrap()
}

/// `top.txt`, `a/x`, `a/b/y`, `c/z`, `d/e/w` on `main`.
fn files_repo(dir: &Path) {
    g(dir, &["init", "-q", "-b", "main"]);
    g(dir, &["config", "core.autocrlf", "false"]);
    g(dir, &["config", "user.name", "Test"]);
    g(dir, &["config", "user.email", "test@example.com"]);
    write(dir, "top.txt", "1\n");
    write(dir, "a/x", "2\n");
    write(dir, "a/b/y", "3\n");
    write(dir, "c/z", "4\n");
    write(dir, "d/e/w", "5\n");
    g(dir, &["add", "-A"]);
    g(dir, &["commit", "-qm", "init"]);
}

/// A cone-mode sparse checkout of `a/b` and `d`: `c/z` is skip-worktree.
fn sparse_repo(name: &str) -> Tmp {
    let t = tmp(name);
    files_repo(t.path());
    g(t.path(), &["sparse-checkout", "set", "--cone", "a/b", "d"]);
    assert!(!t.path().join("c/z").exists());
    t
}

fn status_text(dir: &Path) -> String {
    g(dir, &["status", "--porcelain"])
}

fn flags(dir: &Path) -> String {
    g(dir, &["ls-files", "-t"])
}

/// The working files on disk (not `.git`), sorted.
fn disk(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn status_and_working_copy_agree_with_git_on_a_sparse_checkout() {
    let t = sparse_repo("sparse-status");
    let dir = t.path();
    write(dir, "a/b/y", "changed\n");
    write(dir, "new.txt", "new\n");
    let repo = open(dir);
    let mut ours: Vec<String> = repo
        .status()
        .unwrap()
        .files
        .iter()
        .map(|f| format!("{:?} {}", f.state, f.path))
        .collect();
    ours.sort();
    assert_eq!(ours, ["Modified a/b/y", "Untracked new.txt"]);
    let copy: Vec<String> = repo
        .working_copy()
        .unwrap()
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    assert_eq!(copy, ["/a/b/y", "/new.txt"]);
    assert_eq!(status_text(dir), " M a/b/y\n?? new.txt\n");
}

#[test]
fn staging_and_committing_never_delete_a_skipped_file() {
    let t = sparse_repo("sparse-commit");
    let dir = t.path();
    write(dir, "a/b/y", "changed\n");
    let repo = open(dir);
    repo.stage(&paths(&["/c/z", "/a/b/y"])).unwrap();
    assert_eq!(status_text(dir), "M  a/b/y\n");
    repo.commit("edit", &paths(&["c/z", "a/b/y"]), None)
        .unwrap();
    assert_eq!(
        g(dir, &["ls-tree", "-r", "--name-only", "HEAD"])
            .lines()
            .count(),
        5
    );
    assert_eq!(status_text(dir), "");
    assert!(flags(dir).contains("S c/z"));
    // The commit action with the staged files keeps it too.
    write(dir, "d/e/w", "6\n");
    repo.stage(&paths(&["/d/e/w"])).unwrap();
    repo.apply(
        &GitAction::Commit {
            message: "staged".into(),
            paths: Vec::new(),
            staged: true,
        },
        &ActionOptions {
            unsaved: false,
            credentials: &NoCredentials,
            identity: None,
        },
    )
    .unwrap();
    assert_eq!(status_text(dir), "");
    assert!(flags(dir).contains("S c/z"));
    assert!(g(dir, &["ls-tree", "-r", "--name-only", "HEAD"]).contains("c/z"));
}

#[test]
fn capture_and_set_state_keep_the_flags() {
    let t = sparse_repo("sparse-state");
    let dir = t.path();
    let repo = open(dir);
    let before = flags(dir);
    let state = repo
        .capture_working_state(&CaptureOptions::default())
        .unwrap();
    assert!(state.changed.is_empty(), "{:?}", state.changed);
    write(dir, "a/x", "edited\n");
    let now = repo
        .capture_working_state(&CaptureOptions::default())
        .unwrap();
    repo.set_state(
        &StateTarget {
            refs: None,
            files: state.files.clone(),
            index: state.index.clone(),
            conflicts: None,
            merge: None,
            keep: Vec::new(),
        },
        &StateExpected {
            refs: None,
            files: now.files,
        },
    )
    .unwrap();
    assert_eq!(flags(dir), before);
    assert_eq!(status_text(dir), "");
    assert!(!dir.join("c/z").exists());
}

#[test]
fn switching_branches_keeps_skipped_files_off_the_disk() {
    let t = tmp("sparse-switch");
    let dir = t.path();
    files_repo(dir);
    g(dir, &["switch", "-q", "-c", "other"]);
    write(dir, "c/z", "other\n");
    g(dir, &["commit", "-qam", "c/z on other"]);
    g(dir, &["switch", "-q", "main"]);
    g(dir, &["sparse-checkout", "set", "--cone", "a/b", "d"]);
    assert_eq!(status_text(dir), "");
    let repo = open(dir);
    repo.checkout("other").unwrap();
    assert_eq!(status_text(dir), "");
    assert!(flags(dir).contains("S c/z"));
    assert!(!dir.join("c/z").exists());
    // Undo of the switch keeps them as well.
    let done = repo
        .apply(
            &GitAction::Switch {
                target: "main".into(),
            },
            &ActionOptions {
                unsaved: false,
                credentials: &NoCredentials,
                identity: None,
            },
        )
        .unwrap();
    assert_eq!(status_text(dir), "");
    let undo = done.undo.expect("undoable");
    repo.undo(&undo.id, false).unwrap();
    assert_eq!(g(dir, &["branch", "--show-current"]).trim(), "other");
    assert_eq!(status_text(dir), "");
    assert!(flags(dir).contains("S c/z"));
}

/// Skip-worktree in cone mode of `dirs`: everything except files at the top, below a listed
/// folder, or directly in a parent of one.
fn cone(dirs: &[&str]) -> impl Fn(&str) -> bool + use<> {
    let dirs: Vec<String> = dirs.iter().map(|d| d.to_string()).collect();
    move |path: &str| {
        let parent = path.rsplit_once('/').map_or("", |(p, _)| p);
        let included = parent.is_empty()
            || dirs.iter().any(|d| {
                path.starts_with(&format!("{d}/")) || d.starts_with(&format!("{parent}/"))
            });
        !included
    }
}

#[test]
fn set_skip_worktree_equals_git_sparse_checkout() {
    let ours = tmp("sparse-ours");
    let theirs = tmp("sparse-theirs");
    files_repo(ours.path());
    files_repo(theirs.path());
    g(
        theirs.path(),
        &["sparse-checkout", "set", "--cone", "a/b", "d"],
    );
    let repo = open(ours.path());
    let change = repo.set_skip_worktree(&cone(&["a/b", "d"])).unwrap();
    assert_eq!(change.hidden, ["/c/z"]);
    assert!(change.shown.is_empty());
    assert_eq!(flags(ours.path()), flags(theirs.path()));
    assert_eq!(disk(ours.path()), disk(theirs.path()));
    // Showing everything again equals `git sparse-checkout disable`.
    g(theirs.path(), &["sparse-checkout", "disable"]);
    let change = repo.set_skip_worktree(&|_| false).unwrap();
    assert_eq!(change.shown, ["/c/z"]);
    assert_eq!(flags(ours.path()), flags(theirs.path()));
    assert_eq!(disk(ours.path()), disk(theirs.path()));
    assert_eq!(status_text(ours.path()), "");
}

#[test]
fn set_skip_worktree_refuses_local_changes_and_changes_nothing() {
    let t = tmp("sparse-blocked");
    let dir = t.path();
    files_repo(dir);
    write(dir, "c/z", "mine\n");
    let index = std::fs::read(dir.join(".git/index")).unwrap();
    let err = open(dir)
        .set_skip_worktree(&cone(&["a/b", "d"]))
        .unwrap_err();
    assert!(err.to_string().contains("c/z"), "{err}");
    assert_eq!(std::fs::read(dir.join(".git/index")).unwrap(), index);
    assert_eq!(read(dir, "c/z"), "mine\n");
    assert_eq!(read(dir, "d/e/w"), "5\n");
}

#[test]
fn a_registered_submodule_is_staged_and_committed_as_a_gitlink() {
    let t = tmp("sparse-gitlink");
    let lib = t.path().join("lib");
    let app = t.path().join("app");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::create_dir_all(&app).unwrap();
    files_repo(&lib);
    files_repo(&app);
    g(
        &app,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &lib.to_string_lossy(),
            "lib",
        ],
    );
    g(&app, &["commit", "-qm", "add lib"]);
    let sub = app.join("lib");
    write(&sub, "more.txt", "more\n");
    g(&sub, &["add", "-A"]);
    g(&sub, &["commit", "-qm", "more"]);
    let new = g(&sub, &["rev-parse", "HEAD"]).trim().to_owned();
    let recorded = g(&app, &["rev-parse", "HEAD:lib"]).trim().to_owned();
    assert_eq!(status_text(&app), " M lib\n");
    let repo = open(&app);
    repo.stage(&paths(&["/lib"])).unwrap();
    assert_eq!(status_text(&app), "M  lib\n");
    repo.unstage(&paths(&["/lib"])).unwrap();
    assert_eq!(status_text(&app), " M lib\n");
    assert_eq!(
        g(&app, &["ls-files", "-s", "lib"])
            .split_whitespace()
            .nth(1),
        Some(recorded.as_str())
    );
    repo.commit("new pointer", &paths(&["lib"]), None).unwrap();
    assert_eq!(g(&app, &["rev-parse", "HEAD:lib"]).trim(), new);
    assert_eq!(status_text(&app), "");
    // A nested repository that is no submodule stays refused.
    let nested = app.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    files_repo(&nested);
    assert!(repo.stage(&paths(&["/nested"])).is_err());
}

/// A content filter (seam N11) that stores `clean:` before the bytes and takes it away again.
#[derive(Default)]
struct Prefix {
    smudged: std::sync::Mutex<Vec<String>>,
    prepared: std::sync::Mutex<Vec<String>>,
}

impl ContentFilter for Prefix {
    fn handles(&self, name: &str) -> bool {
        name == "prefix"
    }
    fn clean(&self, _file: &FilterFile<'_>, data: Vec<u8>) -> GitResult<Vec<u8>> {
        if data.starts_with(b"clean:") {
            return Ok(data);
        }
        Ok([b"clean:".as_slice(), &data].concat())
    }
    fn smudge(&self, file: &FilterFile<'_>, data: Vec<u8>) -> GitResult<Vec<u8>> {
        self.smudged.lock().unwrap().push(file.path.to_owned());
        Ok(data
            .strip_prefix(b"clean:")
            .map_or(data.clone(), <[u8]>::to_vec))
    }
    fn prepare_smudge(&self, files: &[(FilterFile<'_>, &[u8])]) -> GitResult<()> {
        let mut p = self.prepared.lock().unwrap();
        p.extend(files.iter().map(|(f, _)| f.path.to_owned()));
        Ok(())
    }
}

#[test]
fn sparse_checkout_with_a_content_filter_cleans_smudges_and_keeps_skipped_files() {
    let commit = |repo: &Repo, paths: &[&str]| {
        repo.apply(
            &GitAction::Commit {
                message: "filtered".into(),
                paths: paths.iter().map(|p| (*p).to_owned()).collect(),
                staged: false,
            },
            &ActionOptions {
                unsaved: false,
                credentials: &NoCredentials,
                identity: None,
            },
        )
        .unwrap();
    };
    let t = tmp("sparse-filter");
    let dir = t.path();
    files_repo(dir);
    write(dir, ".gitattributes", "*.big filter=prefix -text\n");
    write(dir, "c/data.big", "payload\n");
    write(dir, "a/b/more.big", "more\n");
    let filter = std::sync::Arc::new(Prefix::default());
    let mut o = opts();
    o.profile.filter = Some(filter.clone() as std::sync::Arc<dyn ContentFilter>);
    let repo = Repo::open_repository(dir, &o).unwrap().unwrap();
    commit(&repo, &["/.gitattributes", "/c/data.big", "/a/b/more.big"]);
    assert_eq!(
        g(dir, &["cat-file", "-p", "HEAD:c/data.big"]),
        "clean:payload\n"
    );
    assert!(repo.status().unwrap().files.is_empty());

    // Hiding a filtered file: on disk it is smudged, cleaned it equals the index, no refusal.
    repo.set_skip_worktree(&cone(&["a/b", "d"])).unwrap();
    assert!(!dir.join("c/data.big").exists());
    assert!(flags(dir).contains("S c/data.big\n"), "{}", flags(dir));
    assert!(repo.status().unwrap().files.is_empty());
    // A commit next to it keeps the skipped filtered file as stored.
    write(dir, "a/b/more.big", "changed\n");
    commit(&repo, &["/a/b/more.big"]);
    assert_eq!(
        g(dir, &["cat-file", "-p", "HEAD:a/b/more.big"]),
        "clean:changed\n"
    );
    assert_eq!(
        g(dir, &["cat-file", "-p", "HEAD:c/data.big"]),
        "clean:payload\n"
    );
    assert!(flags(dir).contains("S c/data.big\n"));

    // Showing it again writes it like a checkout: prepared in a batch, then smudged.
    filter.smudged.lock().unwrap().clear();
    filter.prepared.lock().unwrap().clear();
    repo.set_skip_worktree(&|_| false).unwrap();
    assert_eq!(read(dir, "c/data.big"), "payload\n");
    let prepared = filter.prepared.lock().unwrap().clone();
    let smudged = filter.smudged.lock().unwrap().clone();
    assert!(
        prepared.contains(&"c/data.big".to_owned()),
        "{prepared:?} {smudged:?}"
    );
    assert!(smudged.contains(&"c/data.big".to_owned()), "{smudged:?}");
    assert!(repo.status().unwrap().files.is_empty());
    // (Real `git` does not know the product's filter, so only the flags are compared with it.)
    assert!(!flags(dir).contains("S "), "{}", flags(dir));
}

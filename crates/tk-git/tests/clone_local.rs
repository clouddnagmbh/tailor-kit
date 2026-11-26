// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Authors

//! Cloning a repository on this machine: bare or normal, by absolute path, `file://`,
//! `~/…` and relative paths. No network, no `git` program.

mod common;

use std::path::Path;

use common::*;
use tk_git::GitError;
use tk_git::{NoCredentials, clone_with_home, expand_home};

/// A bare repository `<holder>/<name>` with one commit (`a.yml`) on `main`.
fn bare_origin(holder: &Path, name: &str) -> std::path::PathBuf {
    let author = tmp("cl-author");
    new_repo(author.path(), "Ada");
    write(author.path(), "a.yml", "one\n");
    let origin = holder.join(name);
    std::fs::create_dir_all(&origin).unwrap();
    bare(&origin);
    add_remote(author.path(), origin.to_str().unwrap());
    let work = reopen(author.path());
    work.commit("first", &paths(&["a.yml"]), None).unwrap();
    work.push(&NoCredentials).unwrap();
    origin
}

fn cloned_ok(dir: &Path, name: &str) {
    assert_eq!(dir.file_name().unwrap(), name);
    assert_eq!(read(dir, "a.yml"), "one\n");
    let repo = reopen(dir);
    assert_eq!(repo.log(1).unwrap()[0].summary, "first");
}

#[test]
fn bare_repository_by_absolute_path_strips_dot_git() {
    let holder = tmp("cl-holder");
    let origin = bare_origin(holder.path(), "playground-origin.git");
    let parent = tmp("cl-parent");
    let dir = clone_with_home(
        origin.to_str().unwrap(),
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
    )
    .unwrap();
    cloned_ok(&dir, "playground-origin");
}

#[test]
fn file_url() {
    let holder = tmp("cl-holder-url");
    let origin = bare_origin(holder.path(), "via-url.git");
    let parent = tmp("cl-parent-url");
    let url = format!("file://{}", origin.to_str().unwrap());
    let dir = clone_with_home(&url, parent.path(), None, &NoCredentials, &opts()).unwrap();
    cloned_ok(&dir, "via-url");
}

#[test]
fn tilde_and_home_relative_paths() {
    let home = tmp("cl-home");
    bare_origin(&home.path().join("repos"), "tilde.git");
    let parent = tmp("cl-parent-tilde");
    let dir = clone_with_home(
        "~/repos/tilde.git",
        parent.path(),
        Some(home.path()),
        &NoCredentials,
        &opts(),
    )
    .unwrap();
    cloned_ok(&dir, "tilde");

    // Relative to the home folder when it does not exist next to the target.
    let parent = tmp("cl-parent-rel");
    let dir = clone_with_home(
        "repos/tilde.git",
        parent.path(),
        Some(home.path()),
        &NoCredentials,
        &opts(),
    )
    .unwrap();
    cloned_ok(&dir, "tilde");
}

#[test]
fn normal_repository_with_working_tree() {
    let author = tmp("cl-normal");
    new_repo(author.path(), "Ada");
    write(author.path(), "a.yml", "one\n");
    reopen(author.path())
        .commit("first", &paths(&["a.yml"]), None)
        .unwrap();
    let parent = tmp("cl-parent-normal");
    let dir = clone_with_home(
        author.path().to_str().unwrap(),
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
    )
    .unwrap();
    assert_eq!(dir.file_name(), author.path().file_name());
    assert_eq!(read(&dir, "a.yml"), "one\n");
    // Pointing at the `.git` folder gives the same name as the working folder.
    let parent = tmp("cl-parent-dotgit");
    let dir = clone_with_home(
        author.path().join(".git").to_str().unwrap(),
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
    )
    .unwrap();
    assert_eq!(dir.file_name(), author.path().file_name());
}

#[test]
fn clear_errors() {
    let parent = tmp("cl-parent-err");
    let plain = tmp("cl-plain");
    let msg = |e: GitError| match e {
        GitError::Failed(m) => m,
        other => panic!("expected GitError::Failed, got {other:?}"),
    };
    let e = clone_with_home(
        plain.path().to_str().unwrap(),
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
    )
    .unwrap_err();
    assert!(msg(e).contains("is not a Git repository"));
    let e = clone_with_home(
        "/no/such/repo.git",
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
    )
    .unwrap_err();
    assert!(msg(e).contains("does not exist"));
    let e = clone_with_home("~/x.git", parent.path(), None, &NoCredentials, &opts()).unwrap_err();
    assert!(msg(e).contains("home folder"));
    // Nothing was left behind.
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn expands_tilde() {
    let home = Path::new("/h/me");
    assert_eq!(
        expand_home("~/a/b.git", Some(home)).unwrap(),
        "/h/me/a/b.git"
    );
    assert_eq!(expand_home("~", Some(home)).unwrap(), "/h/me");
    assert_eq!(
        expand_home("file://~/a.git", Some(home)).unwrap(),
        "file:///h/me/a.git"
    );
    assert_eq!(expand_home("/x/~y", Some(home)).unwrap(), "/x/~y");
    assert_eq!(expand_home("~other/x", Some(home)).unwrap(), "~other/x");
}

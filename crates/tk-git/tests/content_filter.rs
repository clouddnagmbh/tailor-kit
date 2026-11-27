// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The content filter seam (N11, GitTailor wave 3 E-1): a product's `ContentFilter` cleans and
//! smudges the files whose `filter` attribute it handles when the core commits, checks out,
//! reads a blob, computes the status and captures or restores a snapshot; Git's driver for that
//! name is not run; without a filter (or for other names) every byte is as before.
//!
//! The test filter `store` works like Git LFS: clean keeps the content in `.git/store/<key>`
//! and stores a small `ref <key>` text; smudge puts the content back, or leaves the reference
//! when the store lacks it (offline).

mod common;

use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use common::*;
use tk_git::state::{CaptureOptions, StateExpected, StateTarget};
use tk_git::{ActionOptions, NoCredentials, Options, Repo};
use tk_git_types::GitResult;
use tk_git_types::actions::GitAction;
use tk_git_types::filter::{ContentFilter, FilterFile};

#[derive(Default)]
struct Store {
    prepared: Mutex<Vec<String>>,
    smudged: Mutex<Vec<String>>,
}

fn key(data: &[u8]) -> String {
    // FNV-1a: enough to tell the test contents apart.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

impl ContentFilter for Store {
    fn handles(&self, name: &str) -> bool {
        name == "store"
    }
    fn clean(&self, file: &FilterFile<'_>, data: Vec<u8>) -> GitResult<Vec<u8>> {
        assert_eq!(file.name, "store");
        if data.starts_with(b"ref ") {
            return Ok(data);
        }
        let k = key(&data);
        let dir = file.common_dir.join("store");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&k), &data).unwrap();
        Ok(format!("ref {k}\n").into_bytes())
    }
    fn smudge(&self, file: &FilterFile<'_>, data: Vec<u8>) -> GitResult<Vec<u8>> {
        self.smudged.lock().unwrap().push(file.path.to_owned());
        let Some(k) = std::str::from_utf8(&data)
            .ok()
            .and_then(|t| t.strip_prefix("ref "))
            .map(str::trim)
        else {
            return Ok(data);
        };
        Ok(std::fs::read(file.common_dir.join("store").join(k)).unwrap_or(data))
    }
    fn prepare_smudge(&self, files: &[(FilterFile<'_>, &[u8])]) -> GitResult<()> {
        let mut p = self.prepared.lock().unwrap();
        for (f, data) in files {
            assert!(data.starts_with(b"ref "), "{}", f.path);
            p.push(f.path.to_owned());
        }
        Ok(())
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// `main` with `plain.txt`; the branch `bare` stays there. `.gitattributes` routes `*.big`
/// through `store` and `*.other` through a filter nobody handles.
fn fixture(name: &str) -> Tmp {
    let t = tmp(name);
    git(t.path(), &["init", "-q", "-b", "main"]);
    git(t.path(), &["config", "core.autocrlf", "false"]);
    git(t.path(), &["config", "user.name", "Test"]);
    git(t.path(), &["config", "user.email", "test@example.com"]);
    write(t.path(), "plain.txt", "plain\n");
    git(t.path(), &["add", "-A"]);
    git(t.path(), &["commit", "-qm", "init"]);
    git(t.path(), &["branch", "bare"]);
    write(
        t.path(),
        ".gitattributes",
        "*.big filter=store -text\n*.other filter=other\n",
    );
    t
}

fn options(filter: &Arc<Store>) -> Options {
    let mut o = opts();
    o.profile.filter = Some(filter.clone() as Arc<dyn ContentFilter>);
    o
}

fn open(dir: &Path, o: &Options) -> Repo {
    Repo::open_repository(dir, o).unwrap().unwrap()
}

fn apply(repo: &Repo, action: &GitAction) {
    let p = repo.plan(action, false).unwrap();
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    repo.apply(
        action,
        &ActionOptions {
            unsaved: false,
            credentials: &NoCredentials,
            identity: None,
        },
    )
    .unwrap();
}

fn commit(repo: &Repo, paths: &[&str]) {
    apply(
        repo,
        &GitAction::Commit {
            message: "add files".into(),
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
            staged: false,
        },
    );
}

fn switch(repo: &Repo, target: &str) {
    apply(
        repo,
        &GitAction::Switch {
            target: target.into(),
        },
    );
}

fn stored(dir: &Path, rev_path: &str) -> String {
    git(dir, &["cat-file", "-p", rev_path])
}

fn changed(repo: &Repo) -> Vec<String> {
    repo.status()
        .unwrap()
        .files
        .into_iter()
        .map(|f| f.path)
        .collect()
}

#[test]
fn content_filter_commit_stores_the_cleaned_bytes_and_status_stays_clean() {
    if !have_git() {
        return;
    }
    let t = fixture("cf-commit");
    let filter = Arc::new(Store::default());
    let o = options(&filter);
    write(t.path(), "a.big", "a big file\n");
    write(t.path(), "b.other", "other bytes\n");
    let repo = open(t.path(), &o);
    commit(&repo, &["/.gitattributes", "/a.big", "/b.other"]);

    let k = key(b"a big file\n");
    assert_eq!(stored(t.path(), "HEAD:a.big"), format!("ref {k}\n"));
    // A name the filter does not handle (and nobody configured) passes through unchanged.
    assert_eq!(stored(t.path(), "HEAD:b.other"), "other bytes\n");
    assert_eq!(read(t.path(), "a.big"), "a big file\n");
    assert!(t.path().join(".git/store").join(&k).is_file());
    assert_eq!(changed(&open(t.path(), &o)), Vec::<String>::new());

    // Status hashes the cleaned bytes: an edit shows, going back hides it again (also when
    // the index stat no longer matches).
    write(t.path(), "a.big", "edited\n");
    assert_eq!(changed(&open(t.path(), &o)), ["a.big"]);
    let diff = open(t.path(), &o).diff("a.big").unwrap();
    let text = format!("{diff:?}");
    assert!(text.contains(&key(b"edited\n")), "{text}");
    write(t.path(), "a.big", "a big file\n");
    assert_eq!(changed(&open(t.path(), &o)), Vec::<String>::new());

    // Staging cleans as well.
    write(t.path(), "a.big", "staged\n");
    open(t.path(), &o).stage(&["a.big".to_owned()]).unwrap();
    let staged = git(t.path(), &["ls-files", "-s", "a.big"]);
    let id = staged.split_whitespace().nth(1).unwrap();
    assert_eq!(stored(t.path(), id), format!("ref {}\n", key(b"staged\n")));
}

#[test]
fn content_filter_checkout_smudges_and_offline_keeps_the_stored_bytes() {
    if !have_git() {
        return;
    }
    let t = fixture("cf-checkout");
    let filter = Arc::new(Store::default());
    let o = options(&filter);
    write(t.path(), "a.big", "content\n");
    commit(&open(t.path(), &o), &["/.gitattributes", "/a.big"]);

    switch(&open(t.path(), &o), "bare");
    assert!(!t.path().join("a.big").exists());
    filter.prepared.lock().unwrap().clear();
    switch(&open(t.path(), &o), "main");
    assert_eq!(read(t.path(), "a.big"), "content\n");
    assert_eq!(*filter.prepared.lock().unwrap(), ["a.big"]);
    assert_eq!(changed(&open(t.path(), &o)), Vec::<String>::new());

    // The viewers see the content, not the reference.
    let blob = open(t.path(), &o).blob(Some("HEAD"), "/a.big").unwrap();
    assert_eq!(blob.text.as_deref(), Some("content\n"));
    assert_eq!(blob.size, 8);
    assert!(!blob.lfs_pointer);

    // Without the content: the reference stays, never a broken file, and nothing shows as
    // changed.
    switch(&open(t.path(), &o), "bare");
    std::fs::remove_dir_all(t.path().join(".git/store")).unwrap();
    switch(&open(t.path(), &o), "main");
    let k = key(b"content\n");
    assert_eq!(read(t.path(), "a.big"), format!("ref {k}\n"));
    assert_eq!(changed(&open(t.path(), &o)), Vec::<String>::new());
}

#[test]
fn content_filter_snapshots_hold_the_cleaned_bytes_and_restore_smudges() {
    if !have_git() {
        return;
    }
    let t = fixture("cf-state");
    let filter = Arc::new(Store::default());
    let o = options(&filter);
    write(t.path(), "a.big", "first\n");
    let repo = open(t.path(), &o);
    commit(&repo, &["/.gitattributes", "/a.big"]);
    write(t.path(), "a.big", "second\n");
    let repo = open(t.path(), &o);
    let snap = repo
        .capture_working_state(&CaptureOptions::default())
        .unwrap();
    assert_eq!(snap.changed, ["/a.big"]);
    assert_eq!(
        stored(t.path(), &format!("{}:a.big", snap.files)),
        format!("ref {}\n", key(b"second\n"))
    );

    // Back to HEAD's files, then to the snapshot: the content comes back through smudge.
    let head_tree = git(t.path(), &["rev-parse", "HEAD^{tree}"])
        .trim()
        .to_owned();
    let now = repo
        .capture_working_state(&CaptureOptions::default())
        .unwrap();
    repo.set_state(
        &StateTarget {
            refs: None,
            files: head_tree.clone(),
            index: head_tree.clone(),
            conflicts: None,
            merge: None,
            keep: Vec::new(),
        },
        &StateExpected {
            refs: None,
            files: now.files.clone(),
        },
    )
    .unwrap();
    assert_eq!(read(t.path(), "a.big"), "first\n");
    let repo = open(t.path(), &o);
    repo.set_state(
        &StateTarget {
            refs: None,
            files: snap.files.clone(),
            index: snap.index.clone(),
            conflicts: None,
            merge: None,
            keep: Vec::new(),
        },
        &StateExpected {
            refs: None,
            files: head_tree,
        },
    )
    .unwrap();
    assert_eq!(read(t.path(), "a.big"), "second\n");
}

#[test]
fn content_filter_replaces_gits_driver_and_without_it_nothing_changes() {
    if !have_git() {
        return;
    }
    // A driver for the same name that would fail: with the content filter it is never run.
    let t = fixture("cf-driver");
    append_config(
        t.path(),
        "[filter \"store\"]\n\tclean = false\n\tsmudge = false\n\trequired = true\n",
    );
    let filter = Arc::new(Store::default());
    write(t.path(), "a.big", "x\n");
    commit(
        &open(t.path(), &options(&filter)),
        &["/.gitattributes", "/a.big"],
    );
    assert_eq!(
        stored(t.path(), "HEAD:a.big"),
        format!("ref {}\n", key(b"x\n"))
    );

    // The same repository without a content filter: the core stores what Git would (here:
    // the raw bytes, `git hash-object` with no driver configured).
    let u = fixture("cf-none");
    write(u.path(), "a.big", "raw bytes\n");
    let none = open(u.path(), &opts());
    commit(&none, &["/.gitattributes", "/a.big"]);
    let want = git(u.path(), &["hash-object", "--no-filters", "a.big"]);
    assert_eq!(git(u.path(), &["rev-parse", "HEAD:a.big"]), want);
    // A Repo can take the filter itself, too.
    let with = open(u.path(), &opts()).with_content_filter(Some(filter.clone()));
    write(u.path(), "a.big", "raw bytes, edited\n");
    commit(&with, &["/a.big"]);
    assert_eq!(
        stored(u.path(), "HEAD:a.big"),
        format!("ref {}\n", key(b"raw bytes, edited\n"))
    );
}

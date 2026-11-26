// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Authors

//! The working copy, the file tree and file contents of any repository (wave 15, strand B).
//! Fixtures are made in the test; the Git command line is the reference for what a status holds.

mod common;

use std::path::Path;
use std::process::Command;

use common::*;
use tk_git::Repo;
use tk_git_types::files::{BLOB_TEXT_LIMIT, GitEntryKind, GitWorkingCopy, WORKING_COPY_LIMIT};
use tk_git_types::repos::GitOperation;
use tk_git_types::{GitFileChange, GitFileState};

/// `git` with a fixed identity and no user or system configuration.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Ada")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .output()
        .expect("git is installed");
    assert!(
        out.status.success() || args[0] == "merge",
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn repo_at(dir: &Path) -> Repo {
    Repo::require(dir, &opts()).unwrap()
}

/// A repository with a first commit; the files are given as `(path, text)`.
fn fixture(name: &str, files: &[(&str, &str)]) -> (Tmp, Repo) {
    let t = tmp(name);
    git(t.path(), &["init", "-q", "-b", "main"]);
    for (path, text) in files {
        write(t.path(), path, text);
    }
    git(t.path(), &["add", "-A"]);
    git(t.path(), &["commit", "-q", "-m", "first"]);
    let repo = repo_at(t.path());
    (t, repo)
}

fn row<'a>(wc: &'a GitWorkingCopy, path: &str) -> &'a tk_git_types::files::GitWorkingFile {
    wc.files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} not in {:?}", wc.files))
}

fn paths(wc: &GitWorkingCopy) -> Vec<&str> {
    wc.files.iter().map(|f| f.path.as_str()).collect()
}

#[test]
fn changed_new_and_deleted_files_of_the_whole_tree() {
    let (t, repo) = fixture(
        "wc-basic",
        &[
            ("README.md", "hello\n"),
            ("api/a.yml", "a: 1\n"),
            ("api/b.yml", "b: 1\n"),
            ("src/deep/er/x.rs", "fn x() {}\n"),
        ],
    );
    let clean = repo.working_copy().unwrap();
    assert!(clean.files.is_empty());
    assert_eq!(clean.branch.as_deref(), Some("main"));
    assert!(clean.head.is_some() && !clean.detached && !clean.truncated);
    assert_eq!(clean.repository, t.path().to_string_lossy());

    write(t.path(), "api/a.yml", "a: 2\n");
    std::fs::remove_file(t.path().join("api/b.yml")).unwrap();
    write(t.path(), "src/deep/er/new.rs", "fn n() {}\n");
    write(t.path(), "docs/guide.md", "# guide\n");

    let wc = repo.working_copy().unwrap();
    assert_eq!(
        paths(&wc),
        vec![
            "/api/a.yml",
            "/api/b.yml",
            "/docs/guide.md",
            "/src/deep/er/new.rs"
        ]
    );
    assert_eq!(row(&wc, "/api/a.yml").state, GitFileState::Modified);
    assert_eq!(row(&wc, "/api/b.yml").state, GitFileState::Deleted);
    assert_eq!(row(&wc, "/docs/guide.md").state, GitFileState::Untracked);
    // The default model: nothing is staged, everything differs from the index.
    for f in &wc.files {
        assert_eq!(f.staged, None, "{}", f.path);
    }
    assert!(row(&wc, "/api/a.yml").unstaged);
    assert!(
        wc.files
            .iter()
            .all(|f| f.extra == tk_git_types::NoExtra {} && f.from.is_none()),
        "the product names its documents, the core does not"
    );
}

#[test]
fn staged_and_unstaged_are_told_apart() {
    let (t, repo) = fixture(
        "wc-staged",
        &[("a.txt", "1\n"), ("b.txt", "1\n"), ("c.txt", "1\n")],
    );
    write(t.path(), "a.txt", "2\n");
    git(t.path(), &["add", "a.txt"]); // staged only
    write(t.path(), "b.txt", "2\n");
    git(t.path(), &["add", "b.txt"]);
    write(t.path(), "b.txt", "3\n"); // staged and changed again
    write(t.path(), "new.txt", "n\n");
    git(t.path(), &["add", "new.txt"]); // staged new file
    git(t.path(), &["rm", "-q", "c.txt"]); // staged deletion

    let wc = repo.working_copy().unwrap();
    let a = row(&wc, "/a.txt");
    assert_eq!(
        (a.state, a.staged, a.unstaged),
        (GitFileState::Modified, Some(GitFileChange::Modified), false)
    );
    let b = row(&wc, "/b.txt");
    assert_eq!(
        (b.state, b.staged, b.unstaged),
        (GitFileState::Modified, Some(GitFileChange::Modified), true)
    );
    let n = row(&wc, "/new.txt");
    assert_eq!(
        (n.state, n.staged, n.unstaged),
        (GitFileState::Untracked, Some(GitFileChange::Added), false)
    );
    let c = row(&wc, "/c.txt");
    assert_eq!(
        (c.state, c.staged),
        (GitFileState::Deleted, Some(GitFileChange::Deleted))
    );
}

#[test]
fn a_change_that_is_undone_in_the_working_tree_is_not_a_change() {
    let (t, repo) = fixture("wc-undone", &[("a.txt", "1\n")]);
    write(t.path(), "a.txt", "2\n");
    git(t.path(), &["add", "a.txt"]);
    write(t.path(), "a.txt", "1\n"); // back to HEAD's content
    assert!(repo.working_copy().unwrap().files.is_empty());
}

#[test]
fn renames_name_the_old_path_and_keep_both_rows() {
    let (t, repo) = fixture(
        "wc-rename",
        &[("old/name.yml", "same content\n"), ("keep.txt", "k\n")],
    );
    std::fs::create_dir_all(t.path().join("new")).unwrap();
    std::fs::rename(t.path().join("old/name.yml"), t.path().join("new/name.yml")).unwrap();
    let wc = repo.working_copy().unwrap();
    assert_eq!(row(&wc, "/old/name.yml").state, GitFileState::Deleted);
    let moved = row(&wc, "/new/name.yml");
    assert_eq!(moved.from.as_deref(), Some("/old/name.yml"));
    assert_eq!(moved.state, GitFileState::Untracked);

    git(t.path(), &["add", "-A"]);
    let wc = repo.working_copy().unwrap();
    let moved = row(&wc, "/new/name.yml");
    assert_eq!(moved.from.as_deref(), Some("/old/name.yml"));
    assert_eq!(moved.staged, Some(GitFileChange::Renamed));
    assert_eq!(
        row(&wc, "/old/name.yml").staged,
        Some(GitFileChange::Deleted)
    );
}

#[test]
fn ignored_files_never_show_up_unless_tracked() {
    let (t, repo) = fixture(
        "wc-ignored",
        &[(".gitignore", "target/\n*.log\n"), ("src/a.rs", "a\n")],
    );
    write(t.path(), "target/debug/big.bin", "x");
    write(t.path(), "build.log", "x");
    write(t.path(), "src/nested.log", "x");
    write(t.path(), "src/b.rs", "b\n");
    let wc = repo.working_copy().unwrap();
    assert_eq!(paths(&wc), vec!["/src/b.rs"]);

    // Force-added files are tracked, so their changes count.
    git(t.path(), &["add", "-f", "build.log"]);
    git(t.path(), &["commit", "-q", "-m", "log"]);
    write(t.path(), "build.log", "changed");
    let wc = repo.working_copy().unwrap();
    assert_eq!(paths(&wc), vec!["/build.log", "/src/b.rs"]);
}

#[test]
fn nested_repositories_are_not_listed() {
    let (t, repo) = fixture("wc-nested", &[("a.txt", "a\n")]);
    let inner = t.path().join("vendor/inner");
    std::fs::create_dir_all(&inner).unwrap();
    git(&inner, &["init", "-q"]);
    write(&inner, "x.txt", "x");
    assert!(repo.working_copy().unwrap().files.is_empty());
}

#[test]
fn a_merge_that_stopped_shows_conflicts_and_the_operation() {
    let (t, repo) = fixture("wc-conflict", &[("f.txt", "base\n")]);
    git(t.path(), &["checkout", "-q", "-b", "side"]);
    write(t.path(), "f.txt", "side\n");
    git(t.path(), &["commit", "-q", "-am", "side"]);
    git(t.path(), &["checkout", "-q", "main"]);
    write(t.path(), "f.txt", "main\n");
    git(t.path(), &["commit", "-q", "-am", "main"]);
    git(t.path(), &["merge", "side"]);
    let wc = repo.working_copy().unwrap();
    assert_eq!(row(&wc, "/f.txt").state, GitFileState::Conflicted);
    assert_eq!(wc.operation, Some(GitOperation::Merge));
}

#[test]
fn a_detached_head_and_a_repository_without_commits() {
    let (t, repo) = fixture("wc-detached", &[("a.txt", "a\n")]);
    let head = git(t.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    git(t.path(), &["checkout", "-q", "--detach", &head]);
    let wc = repo.working_copy().unwrap();
    assert!(wc.detached);
    assert_eq!(wc.branch, None);
    assert_eq!(wc.head.as_deref(), Some(head.as_str()));

    let empty = tmp("wc-unborn");
    git(empty.path(), &["init", "-q", "-b", "main"]);
    write(empty.path(), "first.txt", "1\n");
    let wc = repo_at(empty.path()).working_copy().unwrap();
    assert_eq!((wc.head.clone(), wc.branch.clone()), (None, None));
    assert_eq!(paths(&wc), vec!["/first.txt"]);
}

#[test]
fn a_submodule_entry_is_a_submodule_not_a_file() {
    let (t, repo) = fixture("wc-sub", &[("a.txt", "a\n")]);
    let head = git(t.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    git(
        t.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{head},libs/sub"),
        ],
    );
    std::fs::create_dir_all(t.path().join("libs/sub")).unwrap();
    let wc = repo.working_copy().unwrap();
    let sub = row(&wc, "/libs/sub");
    assert_eq!(sub.kind, GitEntryKind::Submodule);
    assert_eq!(sub.staged, Some(GitFileChange::Added));
}

#[test]
fn the_list_stops_at_the_limit() {
    let (t, repo) = fixture("wc-limit", &[("a.txt", "a\n")]);
    for i in 0..WORKING_COPY_LIMIT + 5 {
        write(t.path(), &format!("many/f{i:05}.txt"), "x");
    }
    let wc = repo.working_copy().unwrap();
    assert_eq!(wc.files.len(), WORKING_COPY_LIMIT);
    assert!(wc.truncated);
}

#[test]
fn the_files_agree_with_git_status() {
    let (t, repo) = fixture(
        "wc-porcelain",
        &[
            (".gitignore", "*.tmp\n"),
            ("a/one.txt", "1\n"),
            ("a/two.txt", "2\n"),
            ("b/three.txt", "3\n"),
            ("top.txt", "t\n"),
        ],
    );
    write(t.path(), "a/one.txt", "1 changed\n");
    git(t.path(), &["add", "a/one.txt"]);
    write(t.path(), "a/two.txt", "2 changed\n");
    std::fs::remove_file(t.path().join("b/three.txt")).unwrap();
    write(t.path(), "b/untracked.txt", "u\n");
    write(t.path(), "scratch.tmp", "ignored\n");
    write(t.path(), "c/d/e/f.txt", "deep\n");
    git(t.path(), &["rm", "-q", "--cached", "top.txt"]);

    let wc = repo.working_copy().unwrap();
    let ours: std::collections::BTreeSet<String> =
        wc.files.iter().map(|f| f.path.clone()).collect();
    let theirs: std::collections::BTreeSet<String> =
        git(t.path(), &["status", "--porcelain=v1", "-uall"])
            .lines()
            .map(|l| format!("/{}", &l[3..]))
            .collect();
    // `top.txt` is deleted from the index but still on disk: Git lists it too (staged delete
    // and untracked), and so do we, with the staged deletion on the row.
    assert_eq!(ours, theirs);
    assert_eq!(row(&wc, "/top.txt").staged, Some(GitFileChange::Deleted));
}

// --- the file tree ---------------------------------------------------------------------------

#[test]
fn the_file_tree_lists_a_folder_at_a_revision_and_in_the_working_tree() {
    let (t, repo) = fixture(
        "ft",
        &[
            (".gitignore", "*.log\nout/\n"),
            ("src/a.rs", "a\n"),
            ("src/b.rs", "b\n"),
            ("src/sub/c.rs", "c\n"),
            ("top.md", "top\n"),
        ],
    );
    write(t.path(), "src/a.rs", "a changed\n");
    std::fs::remove_file(t.path().join("src/b.rs")).unwrap();
    write(t.path(), "src/new.rs", "n\n");
    write(t.path(), "src/debug.log", "x");
    write(t.path(), "out/o.bin", "x");

    let at_head = repo.file_tree(Some("HEAD"), "/src").unwrap();
    let names: Vec<_> = at_head.iter().map(|e| (e.name.as_str(), e.kind)).collect();
    assert_eq!(
        names,
        vec![
            ("sub", GitEntryKind::Dir),
            ("a.rs", GitEntryKind::File),
            ("b.rs", GitEntryKind::File)
        ]
    );
    assert_eq!(at_head[1].path, "/src/a.rs");
    assert_eq!(at_head[1].size, Some(2));
    assert!(at_head.iter().all(|e| e.state.is_none() && !e.ignored));

    let now = repo.file_tree(None, "src").unwrap();
    let get = |n: &str| now.iter().find(|e| e.name == n).unwrap();
    assert_eq!(get("a.rs").state, Some(GitFileState::Modified));
    assert_eq!(get("b.rs").state, Some(GitFileState::Deleted));
    assert_eq!(get("new.rs").state, Some(GitFileState::Untracked));
    assert!(get("debug.log").ignored && get("debug.log").state.is_none());
    assert_eq!(get("sub").kind, GitEntryKind::Dir);
    assert_eq!(get("sub").state, None);

    let root = repo.file_tree(None, "").unwrap();
    let out = root.iter().find(|e| e.name == "out").unwrap();
    assert!(out.ignored, "an ignored folder is dimmed, not hidden");
    assert!(root.iter().all(|e| e.name != ".git"));
    assert_eq!(
        root.first().map(|e| e.kind),
        Some(GitEntryKind::Dir),
        "folders first"
    );

    assert!(repo.file_tree(Some("HEAD"), "/nope").unwrap().is_empty());
    assert!(repo.file_tree(None, "../etc").is_err());
}

// --- file contents ---------------------------------------------------------------------------

fn write_bytes(dir: &Path, rel: &str, bytes: &[u8]) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, b'I', b'H', b'D', b'R',
];

#[test]
fn blobs_are_text_images_or_metadata() {
    let (t, repo) = fixture("blob", &[("notes.md", "# Notes\nline\n")]);
    write_bytes(t.path(), "img/logo.png", PNG);
    write_bytes(t.path(), "data/blob.bin", &[1, 0, 2, 0, 255]);
    write_bytes(t.path(), "bad.txt", &[0xff, 0xfe, b'a']);
    write(
        t.path(),
        "model.glb",
        "version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 12345\n",
    );
    write(
        t.path(),
        "icon.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n",
    );
    git(t.path(), &["add", "-A"]);
    git(t.path(), &["commit", "-q", "-m", "more"]);

    for rev in [None, Some("HEAD")] {
        let text = repo.blob(rev, "/notes.md").unwrap();
        assert_eq!(text.text.as_deref(), Some("# Notes\nline\n"));
        assert!(!text.binary && text.base64.is_none() && !text.truncated && !text.lfs_pointer);
        assert_eq!(text.size, 13);
        assert_eq!(text.rev.is_some(), rev.is_some());
        assert_eq!(
            text.oid.as_deref(),
            Some(git(t.path(), &["hash-object", "notes.md"]).trim())
        );

        let png = repo.blob(rev, "/img/logo.png").unwrap();
        assert_eq!(png.media_type.as_deref(), Some("image/png"));
        assert!(png.binary && png.text.is_none());
        assert_eq!(png.base64.as_deref(), Some("iVBORw0KGgoAAAANSUhEUg=="));

        let bin = repo.blob(rev, "/data/blob.bin").unwrap();
        assert!(bin.binary && bin.text.is_none() && bin.base64.is_none());
        assert_eq!(bin.size, 5);

        let bad = repo.blob(rev, "/bad.txt").unwrap();
        assert!(bad.binary, "not UTF-8 is not text");

        let lfs = repo.blob(rev, "/model.glb").unwrap();
        assert!(lfs.lfs_pointer);
        assert!(lfs.text.as_deref().unwrap().contains("size 12345"));

        let svg = repo.blob(rev, "/icon.svg").unwrap();
        assert!(!svg.binary && svg.text.is_some() && svg.base64.is_some());
    }
    assert!(repo.blob(None, "/missing.txt").is_err());
    assert!(repo.blob(Some("HEAD"), "/missing.txt").is_err());
    assert!(repo.blob(None, "/img").is_err(), "a folder is not a file");
    assert!(repo.blob(None, "/../x").is_err());
}

#[test]
fn a_file_changed_after_the_commit_differs_by_revision() {
    let (t, repo) = fixture("blob-rev", &[("a.txt", "old\n")]);
    write(t.path(), "a.txt", "new\n");
    assert_eq!(
        repo.blob(None, "/a.txt").unwrap().text.as_deref(),
        Some("new\n")
    );
    assert_eq!(
        repo.blob(Some("HEAD"), "/a.txt").unwrap().text.as_deref(),
        Some("old\n")
    );
}

#[test]
fn big_files_are_metadata_only() {
    let (t, repo) = fixture("blob-big", &[("a.txt", "a\n")]);
    let big = vec![b'x'; usize::try_from(BLOB_TEXT_LIMIT).unwrap() + 1];
    write_bytes(t.path(), "big.txt", &big);
    let b = repo.blob(None, "/big.txt").unwrap();
    assert!(b.truncated && b.text.is_none() && b.base64.is_none());
    assert_eq!(b.size, BLOB_TEXT_LIMIT + 1);
    assert!(!b.binary);
    git(t.path(), &["add", "big.txt"]);
    git(t.path(), &["commit", "-q", "-m", "big"]);
    let b = repo.blob(Some("HEAD"), "/big.txt").unwrap();
    assert!(b.truncated && b.text.is_none());
}

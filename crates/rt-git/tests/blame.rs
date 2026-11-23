// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Wave 15, strand C: `blame` names, for every line of any file, the commit that last changed
//! it, following renames and merges. Compared with `git blame --porcelain` when Git is
//! installed.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use common::*;
use gix::ObjectId;
use gix::objs::tree::EntryKind;
use gix::refs::transaction::PreviousValue;

fn snapshot(
    g: &gix::Repository,
    parents: &[ObjectId],
    files: &[(&str, &[u8])],
    message: &str,
    time: i64,
    who: &str,
) -> ObjectId {
    let mut editor = g.edit_tree(ObjectId::empty_tree(g.object_hash())).unwrap();
    for (path, bytes) in files {
        let id = g.write_blob(bytes).unwrap().detach();
        editor.upsert(*path, EntryKind::Blob, id).unwrap();
    }
    let tree = editor.write().unwrap().detach();
    let sig = gix::actor::Signature {
        name: who.into(),
        email: format!("{}@example.com", who.to_lowercase()).into(),
        time: gix::date::Time::new(time, 0),
    };
    let mut buf = gix::date::parse::TimeBuf::default();
    let sig = sig.to_ref(&mut buf);
    g.new_commit_as(sig, sig, message, tree, parents.iter().copied())
        .unwrap()
        .id
}

fn open(dir: &Path) -> gix::Repository {
    gix::open_opts(dir, gix::open::Options::isolated()).unwrap()
}

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// Final line (1-based) → commit id, from `git blame --porcelain`.
fn git_blame(dir: &Path, rev: &str, path: &str) -> BTreeMap<u32, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["blame", "--porcelain", rev, "--", path])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut map = BTreeMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() >= 3
            && parts[0].len() == 40
            && parts[0].bytes().all(|b| b.is_ascii_hexdigit())
            && let (Ok(_orig), Ok(fin)) = (parts[1].parse::<u32>(), parts[2].parse::<u32>())
        {
            // Only the first line of a group has the count; the others repeat the header.
            map.entry(fin).or_insert_with(|| parts[0].to_owned());
        }
    }
    map
}

fn per_line(blame: &rt_app::git::history::GitBlame) -> BTreeMap<u32, String> {
    let mut map = BTreeMap::new();
    for h in &blame.hunks {
        for i in 0..h.lines {
            map.insert(h.start + i, h.commit.oid.clone());
        }
    }
    map
}

struct Fixture {
    _t: Tmp,
    dir: std::path::PathBuf,
    c1: ObjectId,
    c2: ObjectId,
    c3: ObjectId,
    f1: ObjectId,
    m1: ObjectId,
    merge: ObjectId,
}

/// ```text
///  c1  src/main.txt  a b c d                          (Ada)
///  c2  a B c d e                                      (Grace)
///  c3  renamed to src/lib.txt, d to D                 (Linus)
///  f1  x added on top (branch feature)                (Ada)    m1  e to E (main)   (Grace)
///  merge(m1, f1)                                      (Linus)
/// ```
fn fixture() -> Fixture {
    let t = tmp("blame");
    let dir = t.path().to_path_buf();
    rt_git::init(&dir, &opts()).unwrap();
    set_user(&dir, "Ada");
    let g = open(&dir);
    let c1 = snapshot(
        &g,
        &[],
        &[("src/main.txt", b"a\nb\nc\nd\n"), ("README.md", b"hi\n")],
        "one",
        1_000,
        "Ada",
    );
    let c2 = snapshot(
        &g,
        &[c1],
        &[("src/main.txt", b"a\nB\nc\nd\ne\n"), ("README.md", b"hi\n")],
        "two",
        2_000,
        "Grace",
    );
    let c3 = snapshot(
        &g,
        &[c2],
        &[("src/lib.txt", b"a\nB\nc\nD\ne\n"), ("README.md", b"hi\n")],
        "three",
        3_000,
        "Linus",
    );
    let f1 = snapshot(
        &g,
        &[c3],
        &[
            ("src/lib.txt", b"x\na\nB\nc\nD\ne\n"),
            ("README.md", b"hi\n"),
        ],
        "four feature",
        4_000,
        "Ada",
    );
    let m1 = snapshot(
        &g,
        &[c3],
        &[("src/lib.txt", b"a\nB\nc\nD\nE\n"), ("README.md", b"hi\n")],
        "four main",
        4_500,
        "Grace",
    );
    let merge = snapshot(
        &g,
        &[m1, f1],
        &[
            ("src/lib.txt", b"x\na\nB\nc\nD\nE\n"),
            ("README.md", b"hi\n"),
        ],
        "merge",
        5_000,
        "Linus",
    );
    g.reference("refs/heads/main", merge, PreviousValue::Any, "t")
        .unwrap();
    g.reference("refs/heads/feature", f1, PreviousValue::Any, "t")
        .unwrap();
    Fixture {
        _t: t,
        dir,
        c1,
        c2,
        c3,
        f1,
        m1,
        merge,
    }
}

#[test]
fn blames_every_line_across_renames_and_merges() {
    let f = fixture();
    let repo = reopen(&f.dir);
    let blame = repo.blame("HEAD", "/src/lib.txt").unwrap();
    assert_eq!(blame.path, "/src/lib.txt");
    assert_eq!(blame.rev, f.merge.to_string());
    assert_eq!(blame.lines, 6);
    assert!(!blame.binary);
    // x a B c D E
    let want = [f.f1, f.c1, f.c2, f.c1, f.c3, f.m1];
    let got = per_line(&blame);
    for (i, id) in want.iter().enumerate() {
        assert_eq!(got[&(i as u32 + 1)], id.to_string(), "line {}", i + 1);
    }
    // Hunks cover every line once, in order, and name the file's path in that commit.
    let mut next = 1;
    for h in &blame.hunks {
        assert_eq!(h.start, next);
        next += h.lines;
    }
    assert_eq!(next, 7);
    let first = blame
        .hunks
        .iter()
        .find(|h| f.c1 == h.commit.oid.as_str())
        .unwrap();
    assert_eq!(first.path, "/src/main.txt", "before the rename");
    assert_eq!(first.orig_start, 1);
    assert_eq!(first.commit.author, "Ada");
    if have_git() {
        assert_eq!(got, git_blame(&f.dir, "HEAD", "src/lib.txt"));
    }
}

#[test]
fn blames_an_old_revision_and_collection_relative_paths() {
    let f = fixture();
    let repo = reopen(&f.dir);
    let old = repo.blame(&f.c2.to_string(), "/src/main.txt").unwrap();
    let got = per_line(&old);
    let want = [f.c1, f.c2, f.c1, f.c1, f.c2];
    for (i, id) in want.iter().enumerate() {
        assert_eq!(got[&(i as u32 + 1)], id.to_string(), "line {}", i + 1);
    }
    if have_git() {
        assert_eq!(got, git_blame(&f.dir, &f.c2.to_string(), "src/main.txt"));
    }
    // Without a leading slash the path is relative to the collection (here the repository).
    assert_eq!(repo.blame("HEAD", "src/lib.txt").unwrap().lines, 6);
}

#[test]
fn files_that_are_missing_or_binary() {
    let f = fixture();
    let repo = reopen(&f.dir);
    assert!(repo.blame("HEAD", "/nope.txt").is_err());
    assert!(repo.blame("HEAD", "/../etc/passwd").is_err());
    assert!(repo.blame("nobranch", "/README.md").is_err());

    let g = open(&f.dir);
    let bin = snapshot(
        &g,
        &[f.merge],
        &[("logo.png", b"\x89PNG\0\0\0"), ("src/lib.txt", b"x\n")],
        "binary",
        6_000,
        "Ada",
    );
    g.reference("refs/heads/bin", bin, PreviousValue::Any, "t")
        .unwrap();
    let blame = repo.blame("bin", "/logo.png").unwrap();
    assert!(blame.binary);
    assert!(blame.hunks.is_empty());
    assert_eq!(blame.lines, 0);
}

#[test]
fn an_empty_file_has_no_lines_and_the_last_line_without_newline_counts() {
    let t = tmp("blame-edge");
    let dir = t.path().to_path_buf();
    rt_git::init(&dir, &opts()).unwrap();
    set_user(&dir, "Ada");
    let g = open(&dir);
    let c1 = snapshot(
        &g,
        &[],
        &[("a.txt", b""), ("b.txt", b"one\ntwo")],
        "one",
        1_000,
        "Ada",
    );
    let c2 = snapshot(
        &g,
        &[c1],
        &[("a.txt", b""), ("b.txt", b"one\ntwo\nthree")],
        "two",
        2_000,
        "Grace",
    );
    g.reference("refs/heads/main", c2, PreviousValue::Any, "t")
        .unwrap();
    let repo = reopen(&dir);
    let a = repo.blame("HEAD", "/a.txt").unwrap();
    assert_eq!((a.lines, a.hunks.len()), (0, 0));
    let b = per_line(&repo.blame("HEAD", "/b.txt").unwrap());
    assert_eq!(b[&1], c1.to_string());
    assert_eq!(
        b[&2],
        c2.to_string(),
        "the last line changed (no newline before it)"
    );
    assert_eq!(b[&3], c2.to_string());
    if have_git() {
        assert_eq!(b, git_blame(&dir, "HEAD", "b.txt"));
    }
}

#[test]
fn the_history_of_any_file_follows_renames() {
    use rt_app::git::GitFileChange::*;
    let f = fixture();
    let repo = reopen(&f.dir);
    let log = repo.file_log("/src/lib.txt", 50).unwrap();
    let got: Vec<(String, rt_app::git::GitFileChange, &str, Option<&str>)> = log
        .iter()
        .map(|c| {
            (
                c.commit.oid.clone(),
                c.change,
                c.path.as_str(),
                c.from.as_deref(),
            )
        })
        .collect();
    let id = |o: ObjectId| o.to_string();
    assert_eq!(
        got,
        vec![
            (id(f.merge), Modified, "/src/lib.txt", None),
            (id(f.m1), Modified, "/src/lib.txt", None),
            (id(f.f1), Modified, "/src/lib.txt", None),
            (id(f.c3), Renamed, "/src/lib.txt", Some("/src/main.txt")),
            (id(f.c2), Modified, "/src/main.txt", None),
            (id(f.c1), Added, "/src/main.txt", None),
        ]
    );
    // Not a collection file, and a path without the slash is still the collection's.
    assert_eq!(repo.file_log("/README.md", 10).unwrap().len(), 1);
    assert!(repo.file_log("/nope.txt", 10).unwrap().is_empty());
    assert!(repo.file_log("/../x", 10).is_err());
}

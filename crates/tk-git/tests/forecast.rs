// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! `Repo::merge_forecast`: the merge in memory names the same conflicted files as
//! `git merge-tree --write-tree --name-only` (Git 2.38 and later), and leaves `.git` and the
//! working tree byte for byte the same. The histories are made with the Git command line, the
//! only place a real `git` runs; without one the comparisons are skipped.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use common::*;

/// Runs `git` isolated from the user's configuration; `None` if it is not installed.
fn git_out(dir: &Path, args: &[&str]) -> Option<(bool, String)> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_AUTHOR_NAME", "Ada")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .output()
        .ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let (ok, out) = git_out(dir, args).expect("git");
    assert!(ok, "git {args:?} failed: {out}");
    out
}

/// Whether a `git` with `merge-tree --write-tree` (2.38+) is installed.
fn have_git() -> bool {
    let Some((true, v)) = git_out(Path::new("."), &["--version"]) else {
        eprintln!("SKIPPED: `git` is not installed");
        return false;
    };
    let nums: Vec<u32> = v
        .split_whitespace()
        .nth(2)
        .unwrap_or("0")
        .split('.')
        .take(2)
        .map(|n| n.parse().unwrap_or(0))
        .collect();
    let ok = nums.first().copied().unwrap_or(0) > 2
        || (nums.first() == Some(&2) && nums.get(1).copied().unwrap_or(0) >= 38);
    if !ok {
        eprintln!("SKIPPED: git {v} has no merge-tree --write-tree");
    }
    ok
}

/// Lines of a text file of `n` lines, line `i` reading `<word> i`.
fn lines(word: &str, n: usize) -> String {
    (1..=n).map(|i| format!("{word} {i}\n")).collect()
}

fn with_line(text: &str, line: usize, new: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, l)| {
            if i + 1 == line {
                format!("{new}\n")
            } else {
                format!("{l}\n")
            }
        })
        .collect()
}

/// A repository with `main` at a first commit of `files`, ready for two branches.
fn repo(name: &str, files: &[(&str, &[u8])]) -> Tmp {
    let t = tmp(name);
    git(t.path(), &["init", "-q", "-b", "main"]);
    for (p, bytes) in files {
        let path = t.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    git(t.path(), &["add", "-A"]);
    git(t.path(), &["commit", "-q", "-m", "base"]);
    t
}

/// On a new branch `name` from `from`: writes (`Some`) or deletes (`None`) files, renames
/// (`mv`), commits.
fn branch(
    dir: &Path,
    name: &str,
    from: &str,
    edits: &[(&str, Option<&[u8]>)],
    mv: &[(&str, &str)],
) {
    git(dir, &["checkout", "-q", "-b", name, from]);
    for (a, b) in mv {
        git(dir, &["mv", a, b]);
    }
    for (p, bytes) in edits {
        let path = dir.join(p);
        match bytes {
            Some(b) => {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b).unwrap();
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", name]);
    git(dir, &["checkout", "-q", "main"]);
}

/// The conflicted files `git merge-tree --write-tree --name-only` names, with a leading `/`.
fn merge_tree(dir: &Path, ours: &str, theirs: &str, unrelated: bool) -> Vec<String> {
    let mut args = vec!["merge-tree", "--write-tree", "--name-only"];
    if unrelated {
        args.push("--allow-unrelated-histories");
    }
    args.extend([ours, theirs]);
    let (_, out) = git_out(dir, &args).expect("git");
    out.lines()
        .skip(1)
        .take_while(|l| !l.is_empty())
        .map(|l| format!("/{l}"))
        .collect()
}

/// Every file under `dir` (`.git` included) with its bytes.
fn bytes_of(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn conflicted(f: &tk_git::MergeForecast) -> Vec<String> {
    f.files
        .iter()
        .filter(|f| !f.conflicts.is_empty())
        .map(|f| f.path.clone())
        .collect()
}

/// Forecasts `theirs` into `ours`, checks that nothing on disk changed and that the conflicts
/// equal Git's.
fn check(dir: &Path, ours: &str, theirs: &str, unrelated: bool) -> tk_git::MergeForecast {
    let before = bytes_of(dir);
    let forecast = reopen(dir).merge_forecast(ours, theirs).unwrap();
    assert_eq!(bytes_of(dir), before, "the forecast changed files on disk");
    assert_eq!(
        conflicted(&forecast),
        merge_tree(dir, ours, theirs, unrelated),
        "conflicts of {theirs} into {ours}"
    );
    forecast
}

#[test]
fn conflicts_equal_git_merge_tree() {
    if !have_git() {
        return;
    }
    let text = lines("line", 20);
    let bin: Vec<u8> = (0u8..=255).cycle().take(2000).collect();
    let t = repo(
        "forecast-kinds",
        &[
            ("same.txt", text.as_bytes()),
            ("clean.txt", text.as_bytes()),
            ("moved.txt", lines("moved", 20).as_bytes()),
            ("moved-clash.txt", lines("clash", 20).as_bytes()),
            ("gone.txt", text.as_bytes()),
            ("data.bin", &bin),
            ("only-ours.txt", text.as_bytes()),
            ("dir/nested.txt", text.as_bytes()),
        ],
    );
    let dir = t.path();
    let mut bin_ours = bin.clone();
    bin_ours[10] = 1;
    let mut bin_theirs = bin.clone();
    bin_theirs[10] = 2;
    let ours_same = with_line(&text, 5, "ours 5");
    let ours_clean = with_line(&text, 2, "ours 2");
    let ours_clash = with_line(&lines("clash", 20), 3, "ours 3");
    let ours_only = with_line(&text, 1, "ours 1");
    let ours_nested = with_line(&text, 9, "ours 9");
    branch(
        dir,
        "ours",
        "main",
        &[
            ("same.txt", Some(ours_same.as_bytes())),
            ("clean.txt", Some(ours_clean.as_bytes())),
            ("renamed-clash.txt", Some(ours_clash.as_bytes())),
            ("gone.txt", None),
            ("data.bin", Some(&bin_ours)),
            ("both-new.txt", Some(b"ours\n")),
            ("only-ours.txt", Some(ours_only.as_bytes())),
            ("dir/nested.txt", Some(ours_nested.as_bytes())),
        ],
        &[
            ("moved.txt", "renamed.txt"),
            ("moved-clash.txt", "renamed-clash.txt"),
        ],
    );
    let theirs_same = with_line(&text, 5, "theirs 5");
    let theirs_clean = with_line(&text, 18, "theirs 18");
    let theirs_moved = with_line(&lines("moved", 20), 7, "theirs 7");
    let theirs_clash = with_line(&lines("clash", 20), 3, "theirs 3");
    let theirs_gone = with_line(&text, 4, "theirs 4");
    let theirs_nested = with_line(&text, 9, "theirs 9");
    branch(
        dir,
        "theirs",
        "main",
        &[
            ("same.txt", Some(theirs_same.as_bytes())),
            ("clean.txt", Some(theirs_clean.as_bytes())),
            ("moved.txt", Some(theirs_moved.as_bytes())),
            ("moved-clash.txt", Some(theirs_clash.as_bytes())),
            ("gone.txt", Some(theirs_gone.as_bytes())),
            ("data.bin", Some(&bin_theirs)),
            ("both-new.txt", Some(b"theirs\n")),
            ("dir/nested.txt", Some(theirs_nested.as_bytes())),
            ("only-theirs.txt", Some(b"new\n")),
        ],
        &[],
    );
    let f = check(dir, "ours", "theirs", false);
    // Content, rename with a clash, deleted against changed, binary, new on both sides.
    assert_eq!(
        conflicted(&f),
        [
            "/both-new.txt",
            "/data.bin",
            "/dir/nested.txt",
            "/gone.txt",
            "/renamed-clash.txt",
            "/same.txt"
        ]
    );
    assert!(!f.up_to_date && !f.fast_forward);
    assert!(f.base.is_some());
    let file = |p: &str| f.files.iter().find(|x| x.path == p).unwrap().clone();
    assert_eq!(file("/same.txt").conflicts, ["lines 5-9"]);
    assert_eq!(file("/gone.txt").conflicts, ["whole file"]);
    assert_eq!(file("/data.bin").conflicts, ["whole file"]);
    // Both sides changed it in other places: listed, clean.
    let clean = file("/clean.txt");
    assert!(clean.conflicts.is_empty() && clean.clean == 1, "{clean:?}");
    // A rename on our side, their change on the old name: follows the rename, no conflict
    // (both sides changed the old path, so it is listed, clean).
    assert!(file("/moved.txt").conflicts.is_empty());
    assert!(f.files.iter().all(|x| x.path != "/renamed.txt"));
    assert!(f.incoming.contains(&"/only-theirs.txt".to_owned()));
    assert!(f.incoming.contains(&"/moved.txt".to_owned()));
    assert!(!f.incoming.contains(&"/only-ours.txt".to_owned()));
    // The other way round gives Git's answer too.
    check(dir, "theirs", "ours", false);
}

#[test]
fn clean_merges_up_to_date_and_fast_forward() {
    if !have_git() {
        return;
    }
    let text = lines("line", 10);
    let t = repo(
        "forecast-clean",
        &[("a.txt", text.as_bytes()), ("b.txt", text.as_bytes())],
    );
    let dir = t.path();
    branch(
        dir,
        "ours",
        "main",
        &[("a.txt", Some(with_line(&text, 1, "o").as_bytes()))],
        &[],
    );
    branch(
        dir,
        "theirs",
        "main",
        &[("b.txt", Some(with_line(&text, 1, "t").as_bytes()))],
        &[],
    );
    let f = check(dir, "ours", "theirs", false);
    assert!(f.files.is_empty() && f.conflicts() == 0, "{f:?}");
    assert_eq!(f.incoming, ["/b.txt"]);

    let up = check(dir, "ours", "main", false);
    assert!(up.up_to_date && !up.fast_forward && up.files.is_empty() && up.incoming.is_empty());
    let same = check(dir, "ours", "ours", false);
    assert!(same.up_to_date && !same.fast_forward);

    let ff = check(dir, "main", "theirs", false);
    assert!(ff.fast_forward && !ff.up_to_date && ff.files.is_empty());
    assert_eq!(ff.incoming, ["/b.txt"]);
}

#[test]
fn unrelated_histories_and_several_merge_bases() {
    if !have_git() {
        return;
    }
    let text = lines("line", 10);
    let t = repo("forecast-odd", &[("a.txt", text.as_bytes())]);
    let dir = t.path();
    // Unrelated: a second root that adds a.txt differently.
    git(dir, &["checkout", "-q", "--orphan", "other"]);
    git(dir, &["rm", "-q", "-rf", "."]);
    std::fs::write(dir.join("a.txt"), "other\n").unwrap();
    std::fs::write(dir.join("z.txt"), "z\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "other root"]);
    git(dir, &["checkout", "-q", "main"]);
    let f = check(dir, "main", "other", true);
    assert_eq!(f.base, None);
    assert_eq!(conflicted(&f), ["/a.txt"]);

    // Criss-cross: two merge bases, merged into a virtual one in memory.
    branch(dir, "x", "main", &[("x.txt", Some(b"x\n"))], &[]);
    branch(dir, "y", "main", &[("y.txt", Some(b"y\n"))], &[]);
    git(dir, &["checkout", "-q", "x"]);
    git(dir, &["merge", "-q", "--no-edit", "y~0", "-m", "x takes y"]);
    git(dir, &["checkout", "-q", "y"]);
    git(dir, &["merge", "-q", "--no-edit", "x~1", "-m", "y takes x"]);
    git(dir, &["checkout", "-q", "main"]);
    branch(
        dir,
        "x2",
        "x",
        &[("a.txt", Some(with_line(&text, 2, "x2").as_bytes()))],
        &[],
    );
    branch(
        dir,
        "y2",
        "y",
        &[("a.txt", Some(with_line(&text, 2, "y2").as_bytes()))],
        &[],
    );
    let bases = git(dir, &["merge-base", "--all", "x2", "y2"]);
    assert_eq!(bases.lines().count(), 2, "{bases}");
    let f = check(dir, "x2", "y2", false);
    assert_eq!(conflicted(&f), ["/a.txt"]);
}

#[test]
fn default_branch_and_review_heads() {
    if !have_git() {
        return;
    }
    let t = repo("forecast-refs", &[("a.txt", b"a\n")]);
    let dir = t.path();
    let head = git(dir, &["rev-parse", "HEAD"]).trim().to_owned();
    let r = reopen(dir);
    assert_eq!(
        r.default_branch().unwrap().as_deref(),
        Some("refs/heads/main")
    );
    git(dir, &["branch", "-m", "main", "trunk"]);
    assert_eq!(reopen(dir).default_branch().unwrap(), None);
    git(dir, &["update-ref", "refs/remotes/origin/master", &head]);
    assert_eq!(
        reopen(dir).default_branch().unwrap().as_deref(),
        Some("refs/remotes/origin/master")
    );
    git(dir, &["update-ref", "refs/remotes/origin/trunk", &head]);
    git(
        dir,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );
    assert_eq!(
        reopen(dir).default_branch().unwrap().as_deref(),
        Some("refs/remotes/origin/trunk")
    );

    assert!(reopen(dir).review_heads().unwrap().is_empty());
    git(dir, &["update-ref", "refs/tailor/review/github/12", &head]);
    git(
        dir,
        &["update-ref", "refs/tailor/review/github/12-base", &head],
    );
    git(dir, &["update-ref", "refs/tailor/review/gitlab/3", &head]);
    git(dir, &["update-ref", "refs/other/review/github/4", &head]);
    let heads = reopen(dir).review_heads().unwrap();
    let names: Vec<(&str, u64)> = heads.iter().map(|h| (h.host.as_str(), h.number)).collect();
    assert_eq!(names, [("github", 12), ("gitlab", 3)]);
    assert_eq!(heads[0].reference, "refs/tailor/review/github/12");
    assert_eq!(heads[0].commit, head);
}

#[test]
fn a_forecast_writes_no_object() {
    if !have_git() {
        return;
    }
    let text = lines("line", 10);
    let t = repo("forecast-objects", &[("a.txt", text.as_bytes())]);
    let dir = t.path();
    branch(
        dir,
        "ours",
        "main",
        &[("a.txt", Some(with_line(&text, 3, "o").as_bytes()))],
        &[],
    );
    branch(
        dir,
        "theirs",
        "main",
        &[("a.txt", Some(with_line(&text, 3, "t").as_bytes()))],
        &[],
    );
    let count = || git(dir, &["count-objects", "-v"]);
    let before = count();
    let repo = reopen(dir);
    for _ in 0..3 {
        let f = repo.merge_forecast("ours", "theirs").unwrap();
        assert_eq!(f.conflicts(), 1);
    }
    assert_eq!(count(), before);
    assert!(repo.merge_forecast("ours", "no-such-branch").is_err());
}

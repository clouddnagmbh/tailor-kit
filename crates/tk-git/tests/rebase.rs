// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Rebase as picks in memory (GitTailor wave 3, strand A, gate point A-1 on the kit side): the
//! plan reads only and previews the replay, the run gives the trees, messages and authors
//! `git rebase -i --empty=drop` gives (real `git` is the witness, in copies of the same
//! repository), commits replayed unchanged keep their ids, a step that stops leaves the
//! repository as `git rebase -i` leaves it (so `git rebase --continue` takes over, and a rebase
//! `git` started is continued here), and undo of every step puts refs, files, index and the
//! rebase state back.

mod common;
mod wizard;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use common::*;

use tk_git_types::actions::{GitAction, GitBlockReason};
use tk_git_types::rebase::{GitRebaseStepKind, GitRebaseStop};
use wizard::*;

// ---- helpers ----------------------------------------------------------------------------------

/// `git` with an empty configuration and extra environment; panics when it fails and `ok`.
fn git_env(dir: &Path, args: &[&str], env: &[(&str, &str)], ok: bool) -> (bool, String) {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", dir)
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("git is installed");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if ok && !out.status.success() {
        panic!("git {args:?} failed: {text}");
    }
    (out.status.success(), text)
}

fn g(dir: &Path, args: &[&str]) -> String {
    git_env(dir, args, &[], true).1
}

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn commit_all(dir: &Path, message: &str) -> String {
    g(dir, &["add", "-A"]);
    g(dir, &["commit", "-q", "--allow-empty", "-m", message]);
    g(dir, &["rev-parse", "HEAD"]).trim().to_owned()
}

/// Copies a repository (work tree and `.git`) to a new folder.
fn copy(from: &Path, name: &str) -> Tmp {
    fn go(a: &Path, b: &Path) {
        std::fs::create_dir_all(b).unwrap();
        for e in std::fs::read_dir(a).unwrap() {
            let e = e.unwrap();
            let to = b.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                go(&e.path(), &to);
            } else {
                std::fs::copy(e.path(), to).unwrap();
            }
        }
    }
    let t = tmp(name);
    go(from, t.path());
    t
}

/// Every file under `.git` with its bytes.
fn git_dir_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let root = dir.join(".git");
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

/// Tree, author and message of every commit in `range`, oldest first.
fn history(dir: &Path, range: &str) -> Vec<String> {
    let text = g(
        dir,
        &[
            "log",
            "--reverse",
            "--format=%T %an <%ae> %ad%n%B%x01",
            "--date=raw",
            range,
        ],
    );
    text.split('\u{1}')
        .map(|s| s.trim_start_matches('\n').to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The files of a rebase in progress (`rebase-merge/*` and next to it) and the index.
fn rebase_state_files(dir: &Path) -> String {
    let git = dir.join(".git");
    let mut out = String::new();
    let rm = git.join("rebase-merge");
    if rm.is_dir() {
        let mut names: Vec<_> = std::fs::read_dir(&rm)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for n in names {
            out.push_str(&format!(
                "{n}: {:?}\n",
                std::fs::read_to_string(rm.join(&n)).unwrap_or_default()
            ));
        }
    }
    for n in ["REBASE_HEAD", "MERGE_MSG", "SQUASH_MSG"] {
        if let Ok(t) = std::fs::read_to_string(git.join(n)) {
            out.push_str(&format!("{n}: {t:?}\n"));
        }
    }
    out.push_str(&g(dir, &["ls-files", "-s"]));
    out
}

fn everything(dir: &Path) -> String {
    format!("{}\n{}", state(dir), rebase_state_files(dir))
}

fn rebase(json: &str) -> GitAction {
    act(json)
}

fn onto_main() -> GitAction {
    rebase(r#"{"action":"rebase","onto":"main"}"#)
}

/// `main` (pushed "init" plus "main c"), `feat` from "init" with three commits; on `feat`.
/// With `conflict`, `main` changes the first line of `notes.txt` and so does "feat notes".
fn scene(name: &str, conflict: bool) -> Fixture {
    let f = fixture(name);
    let d = f.dir();
    g(d, &["checkout", "-q", "-b", "feat"]);
    f.write("a.txt", "a1\n");
    commit_all(d, "feat a");
    f.write("b.txt", "b1\n");
    commit_all(d, "feat b\n\nWith a body.");
    f.write("notes.txt", &NOTES.replacen("one", "ONE (feat)", 1));
    commit_all(d, "feat notes");
    g(d, &["checkout", "-q", "main"]);
    f.write("c.txt", "c\n");
    if conflict {
        f.write("notes.txt", &NOTES.replacen("one", "one (main)", 1));
    }
    commit_all(d, "main c");
    g(d, &["checkout", "-q", "feat"]);
    f
}

fn todo_file(dir: &Path, text: &str) -> String {
    let p = dir.join("todo.txt");
    std::fs::write(&p, text).unwrap();
    p.to_string_lossy().into_owned()
}

/// `git rebase -i --empty=drop <args>` with `todo` as its todo (`None`: as Git proposes it).
fn git_rebase(
    dir: &Path,
    args: &[&str],
    todo: Option<&str>,
    env: &[(&str, &str)],
) -> (bool, String) {
    let side = tmp("rebase-side");
    let editor = todo.map(|t| format!("cp {}", todo_file(side.path(), t)));
    let mut all: Vec<(&str, &str)> = env.to_vec();
    if let Some(e) = &editor {
        all.push(("GIT_SEQUENCE_EDITOR", e.as_str()));
    }
    let mut a = vec!["rebase", "-i", "--empty=drop"];
    a.extend_from_slice(args);
    git_env(dir, &a, &all, false)
}

fn id(dir: &Path, rev: &str) -> String {
    g(dir, &["rev-parse", rev]).trim().to_owned()
}

fn backups(dir: &Path) -> Vec<String> {
    g(
        dir,
        &["for-each-ref", "--format=%(refname)", "refs/tailor/backup/"],
    )
    .lines()
    .map(str::to_owned)
    .collect()
}

// ---- the plan ---------------------------------------------------------------------------------

#[test]
fn a1_the_plan_reads_only_and_previews_every_step() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-plan", false);
    let before = git_dir_bytes(f.dir());
    let files = files(f.dir());
    let p = plan(&f.repo(), &onto_main());
    assert_eq!(
        git_dir_bytes(f.dir()),
        before,
        "a plan writes nothing to .git"
    );
    assert_eq!(files, wizard::files(f.dir()));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.undoable && p.backup);
    let r = p.rebase.expect("a preview");
    assert_eq!(r.branch, "feat");
    assert_eq!(r.onto, id(f.dir(), "main"));
    assert_eq!(r.base, id(f.dir(), "main~1"));
    let subjects: Vec<&str> = r.steps.iter().map(|s| s.subject.as_str()).collect();
    assert_eq!(subjects, ["feat a", "feat b", "feat notes"]);
    assert!(
        r.steps
            .iter()
            .all(|s| s.result.is_some() && s.conflicts.is_empty())
    );
    assert_eq!(r.stops_at, None);
    assert_eq!(r.tip.as_deref(), r.steps[2].result.as_deref());
    assert!(
        p.files.iter().any(|f| f == "c.txt" || f == "/c.txt"),
        "{:?}",
        p.files
    );
}

#[test]
fn a1_the_plan_shows_where_a_conflict_stops_it() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-plan-conflict", true);
    let before = git_dir_bytes(f.dir());
    let p = plan(&f.repo(), &onto_main());
    assert_eq!(git_dir_bytes(f.dir()), before);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    let r = p.rebase.unwrap();
    assert_eq!(r.stops_at, Some(2));
    assert_eq!(r.steps[2].conflicts, ["/notes.txt"]);
    assert!(r.steps[2].result.is_none() && r.tip.is_none());
    assert!(r.steps[0].result.is_some() && r.steps[1].result.is_some());
}

// ---- runs against git -------------------------------------------------------------------------

#[test]
fn a1_a_rebase_gives_what_git_gives_and_undo_puts_it_back() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-run", false);
    let witness = copy(f.dir(), "rebase-run-git");
    let (ok, out) = git_rebase(witness.path(), &["main"], None, &[]);
    assert!(ok, "{out}");
    let before = everything(f.dir());
    let old = id(f.dir(), "feat");
    let p = plan(&f.repo(), &onto_main());
    assert!(p.rebase.unwrap().tip.is_some());
    // (The exact ids of the preview are the dry run's: the plan's committer time is "now".)
    let r = apply(&f.repo(), &onto_main());
    assert_eq!(
        history(f.dir(), "main..feat"),
        history(witness.path(), "main..feat")
    );
    assert_eq!(wizard::files(f.dir()), wizard::files(witness.path()));
    assert_eq!(
        g(f.dir(), &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/feat"
    );
    assert!(g(f.dir(), &["status", "--porcelain"]).trim().is_empty());
    assert!(!f.dir().join(".git/rebase-merge").exists());
    assert!(backups(f.dir()).iter().any(|b| b.ends_with("/feat")));
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(id(f.dir(), "feat"), old);
    assert_eq!(everything(f.dir()), before);
}

#[test]
fn a1_commits_replayed_on_the_same_parent_keep_their_ids() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-keep", false);
    let d = f.dir();
    let (a, b, n) = (id(d, "feat~2"), id(d, "feat~1"), id(d, "feat"));
    let base = id(d, "main~1");
    // Nothing changes: nothing to do.
    let same = rebase(&format!(r#"{{"action":"rebase","onto":"{base}"}}"#));
    assert_eq!(reason(&f.repo(), &same), Some(GitBlockReason::NothingToDo));
    // Dropping the last commit keeps the first two as they are.
    let drop = rebase(&format!(
        r#"{{"action":"rebase","onto":"{base}","steps":[{{"kind":"pick","commit":"{a}"}},{{"kind":"pick","commit":"{b}"}},{{"kind":"drop","commit":"{n}"}}]}}"#
    ));
    apply(&f.repo(), &drop);
    assert_eq!(id(d, "feat"), b);
    assert_eq!(id(d, "feat~1"), a);
}

#[test]
fn a1_a_branch_that_is_not_checked_out_moves_without_touching_the_files() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-other", false);
    g(f.dir(), &["checkout", "-q", "main"]);
    let witness = copy(f.dir(), "rebase-other-git");
    git_rebase(witness.path(), &["main", "feat"], None, &[]);
    let files = wizard::files(f.dir());
    apply(
        &f.repo(),
        &rebase(r#"{"action":"rebase","branch":"feat","onto":"main"}"#),
    );
    assert_eq!(wizard::files(f.dir()), files);
    assert_eq!(
        g(f.dir(), &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/main"
    );
    assert_eq!(
        history(f.dir(), "main..feat"),
        history(witness.path(), "main..feat")
    );
}

#[test]
fn a1_a_conflict_stops_in_gits_format_and_git_continues_it() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-stop", true);
    let witness = copy(f.dir(), "rebase-stop-git");
    let (ok, _) = git_rebase(witness.path(), &["main"], None, &[]);
    assert!(!ok, "git stops too");
    let r = apply(&f.repo(), &onto_main());
    assert!(r.undo.is_some());
    let d = f.dir();
    assert!(g(d, &["status"]).contains("interactive rebase in progress"));
    assert!(g(d, &["status"]).contains("both modified:   notes.txt"));
    assert!(g(d, &["status"]).contains("HEAD detached") || g(d, &["branch"]).contains("no branch"));
    assert_eq!(
        history(d, "main..HEAD"),
        history(witness.path(), "main..HEAD")
    );
    assert_eq!(
        f.read("notes.txt"),
        witness_text(witness.path()),
        "the same conflict markers"
    );
    for n in [
        "head-name",
        "onto",
        "orig-head",
        "git-rebase-todo",
        "done",
        "msgnum",
        "end",
        "stopped-sha",
        "message",
        "author-script",
    ] {
        assert!(d.join(".git/rebase-merge").join(n).exists(), "{n}");
        let ours = std::fs::read_to_string(d.join(".git/rebase-merge").join(n)).unwrap();
        let theirs =
            std::fs::read_to_string(witness.path().join(".git/rebase-merge").join(n)).unwrap();
        if !["done", "git-rebase-todo"].contains(&n) {
            assert_eq!(ours.trim(), theirs.trim(), "{n}");
        }
    }
    let s = f
        .repo()
        .rebase_state()
        .unwrap()
        .expect("a rebase in progress");
    assert_eq!(s.stop, GitRebaseStop::Conflict);
    assert_eq!(s.branch.as_deref(), Some("feat"));
    assert_eq!(s.conflicts, ["/notes.txt"]);
    assert_eq!(s.done.len(), 3);
    assert!(s.todo.is_empty());
    // The same resolution on both sides, then `git rebase --continue` on ours.
    for dir in [d, witness.path()] {
        std::fs::write(dir.join("notes.txt"), NOTES.replacen("one", "resolved", 1)).unwrap();
        g(dir, &["add", "notes.txt"]);
        git_env(dir, &["rebase", "--continue"], &[], true);
    }
    assert_eq!(
        history(d, "main..feat"),
        history(witness.path(), "main..feat")
    );
    assert!(!d.join(".git/rebase-merge").exists());
}

fn witness_text(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("notes.txt")).unwrap()
}

#[test]
fn a1_continue_after_a_conflict_gives_what_git_gives_and_each_step_undoes() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-continue", true);
    let d = f.dir();
    let witness = copy(d, "rebase-continue-git");
    git_rebase(witness.path(), &["main"], None, &[]);
    let start = everything(d);
    let r1 = apply(&f.repo(), &onto_main());
    let stopped = f.read("notes.txt");
    std::fs::write(d.join("notes.txt"), NOTES.replacen("one", "resolved", 1)).unwrap();
    // Unresolved in the index: continue waits.
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"rebase-continue"}"#)),
        Some(GitBlockReason::InProgress)
    );
    g(d, &["add", "notes.txt"]);
    let resolved = everything(d);
    let p = plan(&f.repo(), &act(r#"{"action":"rebase-continue"}"#));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert_eq!(p.rebase.as_ref().unwrap().stops_at, None);
    let r2 = apply(&f.repo(), &act(r#"{"action":"rebase-continue"}"#));
    std::fs::write(
        witness.path().join("notes.txt"),
        NOTES.replacen("one", "resolved", 1),
    )
    .unwrap();
    g(witness.path(), &["add", "notes.txt"]);
    git_env(witness.path(), &["rebase", "--continue"], &[], true);
    assert_eq!(
        history(d, "main..feat"),
        history(witness.path(), "main..feat")
    );
    assert_eq!(g(d, &["symbolic-ref", "HEAD"]).trim(), "refs/heads/feat");
    assert!(!d.join(".git/rebase-merge").exists());
    assert!(!d.join(".git/REBASE_HEAD").exists() && !d.join(".git/MERGE_MSG").exists());
    // Undo of continue: the resolved stop again; undo of the start: as before.
    f.repo().undo(&r2.undo.unwrap().id, false).unwrap();
    assert_eq!(everything(d), resolved);
    // The file as the stop left it (undo refuses to overwrite an edited file).
    f.write("notes.txt", &stopped);
    f.repo().undo(&r1.undo.unwrap().id, false).unwrap();
    assert_eq!(everything(d), start);
}

#[test]
fn a1_undo_of_a_stop_puts_everything_back() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-undo-stop", true);
    let before = everything(f.dir());
    let r = apply(&f.repo(), &onto_main());
    assert!(f.dir().join(".git/rebase-merge").exists());
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(everything(f.dir()), before);
    assert!(f.repo().rebase_state().unwrap().is_none());
}

#[test]
fn a1_a_rebase_git_started_is_continued_here() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-git-started", true);
    let d = f.dir();
    let (ok, _) = git_rebase(d, &["main"], None, &[]);
    assert!(!ok);
    let witness = copy(d, "rebase-git-started-git");
    let s = f.repo().rebase_state().unwrap().unwrap();
    assert_eq!(s.stop, GitRebaseStop::Conflict);
    assert_eq!(s.conflicts, ["/notes.txt"]);
    for dir in [d, witness.path()] {
        std::fs::write(dir.join("notes.txt"), NOTES.replacen("one", "resolved", 1)).unwrap();
        g(dir, &["add", "notes.txt"]);
    }
    git_env(witness.path(), &["rebase", "--continue"], &[], true);
    let r = apply(&f.repo(), &act(r#"{"action":"rebase-continue"}"#));
    assert!(r.undo.is_some());
    assert_eq!(
        history(d, "main..feat"),
        history(witness.path(), "main..feat")
    );
    assert_eq!(g(d, &["symbolic-ref", "HEAD"]).trim(), "refs/heads/feat");
}

#[test]
fn a1_skip_leaves_the_stopped_commit_out_as_git_does() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-skip", true);
    let d = f.dir();
    let witness = copy(d, "rebase-skip-git");
    git_rebase(witness.path(), &["main"], None, &[]);
    git_env(witness.path(), &["rebase", "--skip"], &[], true);
    apply(&f.repo(), &onto_main());
    let p = plan(&f.repo(), &act(r#"{"action":"rebase-skip"}"#));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    apply(&f.repo(), &act(r#"{"action":"rebase-skip"}"#));
    assert_eq!(
        history(d, "main..feat"),
        history(witness.path(), "main..feat")
    );
    assert_eq!(wizard::files(d), wizard::files(witness.path()));
    assert!(!d.join(".git/rebase-merge").exists());
}

#[test]
fn a1_edit_and_reword_stop_and_continue_like_git() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-edit", false);
    let d = f.dir();
    let (a, b, n) = (id(d, "feat~2"), id(d, "feat~1"), id(d, "feat"));
    let witness = copy(d, "rebase-edit-git");
    let todo = format!("pick {a}\nedit {b}\nreword {n}\n");
    let side = tmp("rebase-edit-msg");
    let msg = side.path().join("msg");
    std::fs::write(&msg, "notes, reworded\n").unwrap();
    let editor = format!("cp {}", msg.display());
    let (ok, out) = git_rebase(witness.path(), &["main"], Some(&todo), &[]);
    assert!(ok, "{out}");
    std::fs::write(witness.path().join("b.txt"), "b2\n").unwrap();
    g(witness.path(), &["add", "b.txt"]);
    g(witness.path(), &["commit", "-q", "--amend", "--no-edit"]);
    git_env(
        witness.path(),
        &["rebase", "--continue"],
        &[("GIT_EDITOR", &editor)],
        true,
    );

    let steps = format!(
        r#"[{{"kind":"pick","commit":"{a}"}},{{"kind":"edit","commit":"{b}"}},{{"kind":"reword","commit":"{n}"}}]"#
    );
    let action = rebase(&format!(
        r#"{{"action":"rebase","onto":"main","steps":{steps}}}"#
    ));
    assert_eq!(plan(&f.repo(), &action).rebase.unwrap().stops_at, Some(1));
    apply(&f.repo(), &action);
    assert_eq!(
        f.repo().rebase_state().unwrap().unwrap().stop,
        GitRebaseStop::Edit
    );
    assert!(g(d, &["status"]).contains("interactive rebase in progress"));
    f.write("b.txt", "b2\n");
    apply(&f.repo(), &act(r#"{"action":"rebase-continue"}"#));
    let s = f.repo().rebase_state().unwrap().unwrap();
    assert_eq!(s.stop, GitRebaseStop::Message);
    assert_eq!(
        f.repo().rebase_message().unwrap().as_deref(),
        Some("feat notes\n")
    );
    apply(
        &f.repo(),
        &act(r#"{"action":"rebase-continue","message":"notes, reworded"}"#),
    );
    assert_eq!(
        history(d, "main..feat"),
        history(witness.path(), "main..feat")
    );
    assert!(!d.join(".git/rebase-merge").exists());
}

#[test]
fn a1_squash_fixup_reword_with_messages_and_autosquash_match_git() {
    if !have_git() {
        return;
    }
    let f = fixture("rebase-squash");
    let d = f.dir();
    g(d, &["checkout", "-q", "-b", "feat"]);
    f.write("a.txt", "a1\n");
    commit_all(d, "feat a");
    f.write("b.txt", "b1\n");
    commit_all(d, "feat b\n\nBody of b.");
    f.write("a.txt", "a2\n");
    commit_all(d, "fixup! feat a");
    f.write("b.txt", "b2\n");
    commit_all(d, "squash! feat b\n\nMore about b.");
    f.write("a.txt", "a3\n");
    commit_all(d, "amend! feat a\n\nfeat a, amended\n\nNew body.");
    f.write("c.txt", "c\n");
    commit_all(d, "feat c");
    g(d, &["checkout", "-q", "main"]);
    f.write("m.txt", "m\n");
    commit_all(d, "main m");
    g(d, &["checkout", "-q", "feat"]);

    // Autosquash.
    let witness = copy(d, "rebase-squash-git");
    let (ok, out) = git_rebase(witness.path(), &["--autosquash", "main"], None, &[]);
    assert!(ok, "{out}");
    let auto = rebase(r#"{"action":"rebase","onto":"main","autosquash":true}"#);
    let p = plan(&f.repo(), &auto);
    let kinds: Vec<GitRebaseStepKind> = p
        .rebase
        .unwrap()
        .steps
        .iter()
        .map(|s| s.step.kind)
        .collect();
    use GitRebaseStepKind::*;
    assert_eq!(kinds, [Pick, Fixup, Fixup, Pick, Squash, Pick]);
    let copy_ours = copy(d, "rebase-squash-ours");
    apply(&reopen(copy_ours.path()), &auto);
    assert_eq!(
        history(copy_ours.path(), "main..feat"),
        history(witness.path(), "main..feat")
    );

    // An explicit todo: squash with a message, squash without, fixup, reword with a message.
    let ids: Vec<String> = (0..6).rev().map(|i| id(d, &format!("feat~{i}"))).collect();
    let witness = copy(d, "rebase-squash-git-2");
    let todo = format!(
        "reword {}\nsquash {}\npick {}\nfixup {}\nsquash {}\npick {}\n",
        ids[1], ids[0], ids[3], ids[2], ids[5], ids[4]
    );
    let side = tmp("rebase-squash-msg");
    // The editor writes each message in turn: the reword, the first squash chain (given), the
    // second chain keeps Git's combination.
    let script = side.path().join("editor.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nn=$(cat {dir}/n 2>/dev/null || echo 0)\nn=$((n+1))\necho $n > {dir}/n\nif [ $n = 1 ]; then printf 'b, reworded\\n' > \"$1\"; fi\nif [ $n = 2 ]; then printf 'b and a, squashed\\n\\nGiven.\\n' > \"$1\"; fi\n",
            dir = side.path().display()
        ),
    )
    .unwrap();
    git_env(side.path(), &["init", "-q"], &[], true);
    Command::new("chmod")
        .arg("+x")
        .arg(&script)
        .status()
        .unwrap();
    let editor = script.to_string_lossy().into_owned();
    let (ok, out) = git_rebase(
        witness.path(),
        &["main"],
        Some(&todo),
        &[("GIT_EDITOR", &editor)],
    );
    assert!(ok, "{out}");
    let steps = format!(
        r#"[{{"kind":"reword","commit":"{}","message":"b, reworded"}},{{"kind":"squash","commit":"{}","message":"b and a, squashed\n\nGiven."}},{{"kind":"pick","commit":"{}"}},{{"kind":"fixup","commit":"{}"}},{{"kind":"squash","commit":"{}"}},{{"kind":"pick","commit":"{}"}}]"#,
        ids[1], ids[0], ids[3], ids[2], ids[5], ids[4]
    );
    let action = rebase(&format!(
        r#"{{"action":"rebase","onto":"main","steps":{steps}}}"#
    ));
    let r = plan(&f.repo(), &action);
    assert!(r.blocked.is_none(), "{:?}", r.blocked);
    assert_eq!(r.rebase.unwrap().stops_at, None);
    apply(&f.repo(), &action);
    assert_eq!(
        history(d, "main..feat"),
        history(witness.path(), "main..feat")
    );
}

#[test]
fn a1_a_squash_that_conflicts_stops_and_git_continues_it() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-squash-stop", true);
    let d = f.dir();
    let (a, b, n) = (id(d, "feat~2"), id(d, "feat~1"), id(d, "feat"));
    let witness = copy(d, "rebase-squash-stop-git");
    let todo = format!("pick {a}\npick {b}\nsquash {n}\n");
    let (ok, _) = git_rebase(witness.path(), &["main"], Some(&todo), &[]);
    assert!(!ok);
    let steps = format!(
        r#"[{{"kind":"pick","commit":"{a}"}},{{"kind":"pick","commit":"{b}"}},{{"kind":"squash","commit":"{n}"}}]"#
    );
    apply(
        &f.repo(),
        &rebase(&format!(
            r#"{{"action":"rebase","onto":"main","steps":{steps}}}"#
        )),
    );
    for n in ["message-squash", "current-fixups", "amend", "message"] {
        let ours = std::fs::read_to_string(d.join(".git/rebase-merge").join(n)).unwrap();
        let theirs =
            std::fs::read_to_string(witness.path().join(".git/rebase-merge").join(n)).unwrap();
        if n == "amend" {
            continue;
        }
        assert_eq!(ours, theirs, "{n}");
    }
    for dir in [d, witness.path()] {
        std::fs::write(dir.join("notes.txt"), NOTES.replacen("one", "resolved", 1)).unwrap();
        g(dir, &["add", "notes.txt"]);
    }
    git_env(witness.path(), &["rebase", "--continue"], &[], true);
    // Ours continues with git as well.
    git_env(d, &["rebase", "--continue"], &[], true);
    assert_eq!(
        history(d, "main..feat"),
        history(witness.path(), "main..feat")
    );
}

// ---- what blocks ------------------------------------------------------------------------------

#[test]
fn a1_published_protected_local_changes_and_a_rebase_in_progress_block() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-blocks", false);
    let d = f.dir();
    // Published: feat is on the remote.
    let remote = f.remote.path().to_string_lossy().into_owned();
    g(d, &["push", "-q", &remote, "feat:feat"]);
    g(
        d,
        &[
            "fetch",
            "-q",
            &remote,
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    );
    assert_eq!(
        reason(&f.repo(), &onto_main()),
        Some(GitBlockReason::Published)
    );
    let allowed = rebase(r#"{"action":"rebase","onto":"main","allow_published":true}"#);
    let p = plan(&f.repo(), &allowed);
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(!p.warnings.is_empty());
    let protected = f.repo().with_protected(vec!["fe*".into()]);
    assert_eq!(
        protected
            .plan(&allowed, false)
            .unwrap()
            .blocked
            .map(|b| b.reason),
        Some(GitBlockReason::Protected)
    );
    // Local changes.
    f.write("a.txt", "edited\n");
    assert_eq!(
        reason(&f.repo(), &allowed),
        Some(GitBlockReason::LocalChanges)
    );
    g(d, &["checkout", "-q", "--", "a.txt"]);
    // Squash first, a merge commit in the steps.
    let a = id(d, "feat~2");
    let first = rebase(&format!(
        r#"{{"action":"rebase","onto":"main","allow_published":true,"steps":[{{"kind":"squash","commit":"{a}"}}]}}"#
    ));
    assert_eq!(reason(&f.repo(), &first), Some(GitBlockReason::Unsupported));
    // A rebase in progress.
    let f = scene("rebase-blocks-2", true);
    apply(&f.repo(), &onto_main());
    assert_eq!(
        reason(&f.repo(), &onto_main()),
        Some(GitBlockReason::InProgress)
    );
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"pull-rebase"}"#)),
        Some(GitBlockReason::InProgress)
    );
    // Continue and skip without a rebase: nothing to do.
    let f = scene("rebase-blocks-3", false);
    assert_eq!(
        reason(&f.repo(), &act(r#"{"action":"rebase-continue"}"#)),
        Some(GitBlockReason::NothingToDo)
    );
}

// ---- dry run and pull -------------------------------------------------------------------------

#[test]
fn a1_a_dry_run_is_exact_and_accepting_writes_those_commits() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-dry", false);
    assert!(tk_git::dry_run_supported(&onto_main()));
    let before = git_dir_bytes(f.dir());
    let o = f.repo().dry_run(&onto_main(), &options(false)).unwrap();
    assert_eq!(git_dir_bytes(f.dir()), before);
    assert!(o.exact);
    let tip = o
        .refs
        .iter()
        .find(|m| m.name == "refs/heads/feat")
        .and_then(|m| m.after.clone())
        .unwrap();
    let r = f.repo().accept_dry_run(&o, &options(false)).unwrap();
    assert_eq!(id(f.dir(), "feat"), tip);
    assert!(f.read("c.txt") == "c\n");
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    // A conflict: not exact; accepting runs it and stops.
    let f = scene("rebase-dry-conflict", true);
    let o = f.repo().dry_run(&onto_main(), &options(false)).unwrap();
    assert!(!o.exact);
    assert_eq!(o.conflicts, ["/notes.txt"]);
    f.repo().accept_dry_run(&o, &options(false)).unwrap();
    assert!(f.dir().join(".git/rebase-merge").exists());
}

#[test]
fn a1_pull_rebase_gives_what_git_pull_rebase_gives() {
    if !have_git() {
        return;
    }
    let f = fixture("rebase-pull");
    let d = f.dir();
    // A colleague pushes; we commit locally.
    let colleague = tmp("rebase-pull-colleague");
    let remote = f.remote.path().to_string_lossy().into_owned();
    g(colleague.path(), &["clone", "-q", &remote, "c"]);
    let c = colleague.path().join("c");
    set_user(&c, "Bob");
    std::fs::write(c.join("bob.txt"), "bob\n").unwrap();
    commit_all(&c, "bob");
    g(&c, &["push", "-q", "origin", "HEAD:main"]);
    f.write("mine.txt", "mine\n");
    commit_all(d, "mine");
    let witness = copy(d, "rebase-pull-git");
    git_env(
        witness.path(),
        &["pull", "-q", "--rebase", "origin", "main"],
        &[],
        true,
    );
    let p = plan(&f.repo(), &act(r#"{"action":"pull-rebase"}"#));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.network);
    let r = apply(&f.repo(), &act(r#"{"action":"pull-rebase"}"#));
    assert_eq!(
        history(d, "origin/main..main"),
        history(witness.path(), "origin/main..main")
    );
    assert_eq!(id(d, "main~1"), id(witness.path(), "main~1"));
    assert_eq!(wizard::files(d), wizard::files(witness.path()));
    // Up to date afterwards.
    let again = apply(&f.repo(), &act(r#"{"action":"pull-rebase"}"#));
    assert!(again.message.contains("up to date"), "{}", again.message);
    let _ = r;
}

#[test]
fn a1_abort_of_a_stopped_rebase_goes_back_and_its_undo_brings_the_stop_back() {
    if !have_git() {
        return;
    }
    let f = scene("rebase-abort", true);
    let start = everything(f.dir());
    apply(&f.repo(), &onto_main());
    let stopped = everything(f.dir());
    let r = apply(&f.repo(), &act(r#"{"action":"abort"}"#));
    assert_eq!(everything(f.dir()), start);
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(everything(f.dir()), stopped);
    assert_eq!(
        f.repo().rebase_state().unwrap().unwrap().stop,
        GitRebaseStop::Conflict
    );
}

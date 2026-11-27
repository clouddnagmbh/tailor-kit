// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Dry runs (GitTailor wave 2, strand C): `Repo::dry_run` leaves `.git` and the working tree
//! byte for byte as they are (not even a loose object), names the refs with the real ids of the
//! result, the files and the conflicts; `Repo::accept_dry_run` gives exactly the preview (same
//! commit ids, refs, files) with a backup and a journal entry that `Repo::undo` takes back, and is
//! refused once the repository changed since.

mod common;
mod wizard;

use std::collections::BTreeMap;
use std::path::Path;

use common::*;
use tk_git::{DryRunOutcome, Repo};
use tk_git_types::actions::GitAction;
use wizard::*;

/// Every byte under `dir`, `.git` included (the reflog, objects, the index …).
fn bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

fn assert_same(before: &BTreeMap<String, Vec<u8>>, after: &BTreeMap<String, Vec<u8>>, what: &str) {
    let changed: Vec<&String> = before
        .keys()
        .chain(after.keys())
        .filter(|k| before.get(*k) != after.get(*k))
        .collect();
    assert!(changed.is_empty(), "{what} changed {changed:?}");
}

fn dry(repo: &Repo, action: &GitAction) -> DryRunOutcome {
    repo.dry_run(action, &options(false))
        .unwrap_or_else(|e| panic!("{action:?}: {e}"))
}

/// A fixture with `feature` (two commits off `main`) and one more commit on `main`.
fn diverged(name: &str) -> (Fixture, String) {
    let f = fixture(name);
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"feature","start":"main","switch":true}"#),
    );
    f.commit("feature one", &[("feature.txt", "one\n")]);
    let tip = f.commit(
        "feature two",
        &[("notes.txt", &NOTES.replace("ten", "TEN"))],
    );
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    f.commit("main side", &[("main.txt", "main\n")]);
    (f, tip)
}

/// Dry run of `json`: nothing changes, then accepting gives the preview, undo restores.
fn round_trip(f: &Fixture, json: &str) -> DryRunOutcome {
    let action = act(json);
    let before = bytes(f.dir());
    let shown = state(f.dir());
    let d = dry(&f.repo(), &action);
    assert!(d.plan.blocked.is_none(), "{json}: {:?}", d.plan.blocked);
    assert!(d.exact, "{json}");
    assert_same(&before, &bytes(f.dir()), json);
    // Accepting writes exactly the previewed objects and moves the refs to the previewed ids.
    let r = f.repo().accept_dry_run(&d, &options(false)).unwrap();
    let entry = r.undo.clone().expect("journal entry");
    for m in &d.refs {
        assert_eq!(ref_id(f.dir(), &m.name), m.after, "{json}: {}", m.name);
    }
    let repo = gix::open_opts(f.dir(), gix::open::Options::isolated()).unwrap();
    for id in d.object_ids() {
        assert!(
            repo.has_object(gix::ObjectId::from_hex(id.as_bytes()).unwrap()),
            "{id}"
        );
    }
    for b in &entry.backups {
        assert!(has_ref(f.dir(), b), "{b}");
    }
    // Undo puts everything back.
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(state(f.dir()), shown, "{json}: undo");
    d
}

#[test]
fn merge_with_a_merge_commit() {
    let (f, tip) = diverged("dry-merge");
    let d = round_trip(&f, r#"{"action":"merge","from":"feature"}"#);
    assert_eq!(d.refs.len(), 1);
    let new = d.refs[0].after.clone().unwrap();
    let top = &d.graph.commits[0];
    assert_eq!(top.commit.oid, new);
    assert_eq!(top.parents.len(), 2);
    assert_eq!(top.parents[1], tip);
    assert_eq!(d.graph.head.as_deref(), Some(new.as_str()));
    assert!(top.refs.contains(&"refs/heads/main".to_owned()));
    let paths: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["/feature.txt", "/notes.txt"]);
    let diff = f.repo().dry_run_file_diff(&d, "/notes.txt").unwrap();
    assert!(!diff.hunks.is_empty());
}

#[test]
fn accepting_gives_the_preview_files_and_ids() {
    let (f, _) = diverged("dry-accept");
    let d = dry(&f.repo(), &act(r#"{"action":"merge","from":"feature"}"#));
    let r = f.repo().accept_dry_run(&d, &options(false)).unwrap();
    assert!(r.reload);
    let after = ref_id(f.dir(), "refs/heads/main");
    assert_eq!(after, d.refs[0].after);
    assert_eq!(f.read("notes.txt"), NOTES.replace("ten", "TEN"));
    assert_eq!(f.read("feature.txt"), "one\n");
    assert!(f.repo().status().unwrap().files.is_empty());
}

#[test]
fn fast_forward_cherry_pick_revert_and_reset() {
    let f = fixture("dry-ff");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"feature","start":"main","switch":true}"#),
    );
    let tip = f.commit("feature one", &[("feature.txt", "one\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    let d = round_trip(&f, r#"{"action":"merge","from":"feature"}"#);
    assert_eq!(d.refs[0].after.as_deref(), Some(tip.as_str()));

    let d = round_trip(
        &f,
        &format!(r#"{{"action":"cherry-pick","commit":"{tip}"}}"#),
    );
    assert_eq!(d.graph.commits[0].commit.summary, "feature one");
    let last = f.commit("second", &[("notes.txt", "changed\n")]);
    round_trip(&f, &format!(r#"{{"action":"revert","commit":"{last}"}}"#));
    f.write("notes.txt", "dirty\n");
    f.write("new.txt", "untracked\n");
    round_trip(
        &f,
        r#"{"action":"reset","target":"HEAD~1","mode":"keep-changes"}"#,
    );
    let d = round_trip(
        &f,
        r#"{"action":"reset","target":"HEAD~1","mode":"discard"}"#,
    );
    assert!(d.files.iter().any(|x| x.path == "/notes.txt"));
    assert!(!d.left_behind.is_empty());
}

#[test]
fn amend_commit_delete_branch_and_rewrite() {
    let (f, tip) = diverged("dry-amend");
    round_trip(&f, r#"{"action":"amend","message":"main side, reworded"}"#);
    f.write("main.txt", "main, more\n");
    round_trip(&f, r#"{"action":"amend","paths":["/main.txt"]}"#);
    round_trip(
        &f,
        r#"{"action":"commit","message":"more","paths":["/main.txt"]}"#,
    );
    round_trip(
        &f,
        r#"{"action":"delete-branch","name":"feature","force":true}"#,
    );
    f.write("main.txt", "main\n");
    assert_eq!(
        ref_id(f.dir(), "refs/heads/feature").as_deref(),
        Some(tip.as_str())
    );
    // The branch that is not checked out, then the current one (its files follow).
    let main = ref_id(f.dir(), "refs/heads/main").unwrap();
    round_trip(
        &f,
        &format!(r#"{{"action":"rewrite","branch":"feature","tip":"{main}","summary":""}}"#),
    );
    let parent = tip;
    round_trip(
        &f,
        &format!(
            r#"{{"action":"rewrite","branch":"main","tip":"{parent}","summary":"Take feature"}}"#
        ),
    );
}

#[test]
fn pull_merges_what_was_fetched_last_without_downloading() {
    let (f, tip) = diverged("dry-pull");
    let repo = gix::open_opts(f.dir(), gix::open::Options::isolated()).unwrap();
    repo.reference(
        "refs/remotes/origin/main",
        gix::ObjectId::from_hex(tip.as_bytes()).unwrap(),
        gix::refs::transaction::PreviousValue::Any,
        "test: as if fetched",
    )
    .unwrap();
    let d = round_trip(&f, r#"{"action":"pull"}"#);
    assert!(d.warnings.iter().any(|w| w.contains("does not download")));
    assert_eq!(d.graph.commits[0].parents[1], tip);
}

#[test]
fn conflicts_are_named_and_accepting_runs_the_merge_again() {
    let f = fixture("dry-conflict");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"feature","start":"main","switch":true}"#),
    );
    f.commit("theirs", &[("notes.txt", "theirs\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    f.commit("ours", &[("notes.txt", "ours\n")]);
    let before = bytes(f.dir());
    let d = dry(&f.repo(), &act(r#"{"action":"merge","from":"feature"}"#));
    assert_same(&before, &bytes(f.dir()), "conflicting merge");
    assert_eq!(d.conflicts, ["/notes.txt"]);
    assert!(!d.exact);
    let r = f.repo().accept_dry_run(&d, &options(false)).unwrap();
    assert!(matches!(
        r.outcome,
        Some(tk_git_types::GitPullResult::Conflicts { .. })
    ));
}

#[test]
fn accepting_is_refused_once_the_repository_changed() {
    let (f, _) = diverged("dry-stale");
    let d = dry(&f.repo(), &act(r#"{"action":"merge","from":"feature"}"#));
    f.write("notes.txt", "edited since\n");
    let before = bytes(f.dir());
    let e = f.repo().accept_dry_run(&d, &options(false)).unwrap_err();
    assert!(e.to_string().contains("working files changed"), "{e}");
    assert_same(&before, &bytes(f.dir()), "refused accept");
    // A branch that moved counts too.
    f.write("notes.txt", NOTES);
    let d = dry(&f.repo(), &act(r#"{"action":"merge","from":"feature"}"#));
    f.commit("newer", &[("other.txt", "x\n")]);
    let stale = f.repo().dry_run_stale(&d).unwrap().unwrap();
    assert!(stale.contains("branches"), "{stale}");
}

#[test]
fn a_blocked_plan_is_shown_and_not_accepted() {
    let (f, _) = diverged("dry-blocked");
    f.write("notes.txt", "local change\n");
    let before = bytes(f.dir());
    let d = dry(&f.repo(), &act(r#"{"action":"merge","from":"feature"}"#));
    assert!(d.plan.blocked.is_some());
    assert!(d.refs.is_empty());
    assert!(f.repo().accept_dry_run(&d, &options(false)).is_err());
    assert_same(&before, &bytes(f.dir()), "blocked");
}

#[test]
fn staged_commit_and_amend_from_the_index() {
    let (f, _) = diverged("dry-staged");
    f.write("main.txt", "staged\n");
    f.write("notes.txt", "not staged\n");
    f.repo().stage(&paths(&["main.txt"])).unwrap();
    let d = round_trip(
        &f,
        r#"{"action":"commit","message":"staged only","staged":true}"#,
    );
    let listed: Vec<&str> = d.files.iter().map(|x| x.path.as_str()).collect();
    assert_eq!(listed, ["/main.txt"]);
    // (Undoing the commit leaves the change unstaged, so it is staged again.)
    f.repo().stage(&paths(&["main.txt"])).unwrap();
    round_trip(&f, r#"{"action":"amend","staged":true}"#);
}

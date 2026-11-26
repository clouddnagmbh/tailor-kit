// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Authors

//! Backup and undo of the Git wizards' actions (wave 14, strand G): every undoable action
//! runs plan → apply → undo and the repository looks as before (refs, HEAD, configuration,
//! stash list, files); actions that throw work away leave a backup ref first; undo is refused
//! once refs moved or files were edited since; the journal keeps the newest `UNDO_KEEP`.

mod common;
mod wizard;

use common::*;
use tk_git_types::actions::{GitAction, UNDO_KEEP};
use wizard::*;

/// A repository with a second branch `other` (one commit of its own), local changes on
/// `main`, and the state the action list below expects.
fn scene(name: &str) -> Fixture {
    let f = fixture(name);
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#),
    );
    f.commit(
        "other",
        &[(
            "users/get.yml",
            &http_doc("POST", "https://api.test/users/1"),
        )],
    );
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    f.commit("main", &[("notes.txt", "main\n")]);
    f.commit("main 2", &[("notes.txt", "main 2\n")]);
    apply(
        &f.repo(),
        &act(r#"{"action":"create-tag","name":"v1","target":"HEAD"}"#),
    );
    f
}

type Setup = fn(&Fixture) -> GitAction;

#[test]
fn every_action_undoes_to_the_state_before() {
    let cases: Vec<(&str, Setup, bool)> = vec![
        (
            "create-branch",
            |_| act(r#"{"action":"create-branch","name":"x","start":"other","switch":true}"#),
            true,
        ),
        (
            "create-branch-here",
            |_| act(r#"{"action":"create-branch","name":"x","start":"HEAD"}"#),
            false,
        ),
        (
            "switch",
            |_| act(r#"{"action":"switch","target":"other"}"#),
            true,
        ),
        (
            "switch-detached",
            |_| act(r#"{"action":"switch","target":"v1"}"#),
            true,
        ),
        (
            "rename-branch",
            |_| act(r#"{"action":"rename-branch","from":"main","to":"trunk"}"#),
            false,
        ),
        (
            "delete-branch",
            |_| act(r#"{"action":"delete-branch","name":"other","force":true}"#),
            true,
        ),
        (
            "merge",
            |_| act(r#"{"action":"merge","from":"other"}"#),
            true,
        ),
        (
            "merge-ff",
            |f| {
                apply(
                    &f.repo(),
                    &act(
                        r#"{"action":"create-branch","name":"ahead","start":"HEAD","switch":true}"#,
                    ),
                );
                f.commit("ahead", &[("notes.txt", "ahead\n")]);
                apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
                act(r#"{"action":"merge","from":"ahead"}"#)
            },
            true,
        ),
        (
            "cherry-pick",
            |_| act(r#"{"action":"cherry-pick","commit":"other"}"#),
            true,
        ),
        (
            "revert",
            |_| act(r#"{"action":"revert","commit":"HEAD"}"#),
            true,
        ),
        (
            "reset-discard",
            |f| {
                f.write("notes.txt", "uncommitted\n");
                f.write("new.txt", "untracked\n");
                act(r#"{"action":"reset","target":"HEAD~2","mode":"discard"}"#)
            },
            true,
        ),
        (
            "reset-keep",
            |_| act(r#"{"action":"reset","target":"HEAD~1","mode":"keep-changes"}"#),
            true,
        ),
        (
            "discard",
            |f| {
                f.write("notes.txt", "uncommitted\n");
                f.write(
                    "users/get.yml",
                    &http_doc("DELETE", "https://api.test/users/1"),
                );
                f.write("new.txt", "untracked\n");
                act(r#"{"action":"discard","paths":[]}"#)
            },
            true,
        ),
        (
            "discard-one",
            |f| {
                f.write("notes.txt", "uncommitted\n");
                f.write("new.txt", "untracked\n");
                act(r#"{"action":"discard","paths":["/new.txt"]}"#)
            },
            true,
        ),
        (
            "stash",
            |f| {
                f.write("notes.txt", "uncommitted\n");
                f.write("new.txt", "untracked\n");
                act(r#"{"action":"stash","message":"wip"}"#)
            },
            false,
        ),
        (
            "unstash",
            |f| {
                f.write("notes.txt", "uncommitted\n");
                f.write("new.txt", "untracked\n");
                apply(&f.repo(), &act(r#"{"action":"stash","message":"wip"}"#));
                act(r#"{"action":"unstash","index":0}"#)
            },
            true,
        ),
        (
            "drop-stash",
            |f| {
                f.write("notes.txt", "uncommitted\n");
                apply(&f.repo(), &act(r#"{"action":"stash","message":"wip"}"#));
                act(r#"{"action":"drop-stash","index":0}"#)
            },
            true,
        ),
        (
            "create-tag",
            |_| act(r#"{"action":"create-tag","name":"v2","target":"HEAD~1","message":"Two"}"#),
            false,
        ),
        (
            "delete-tag",
            |_| act(r#"{"action":"delete-tag","name":"v1"}"#),
            true,
        ),
        (
            "add-remote",
            |_| act(r#"{"action":"add-remote","name":"backup","url":"https://example.com/x.git"}"#),
            false,
        ),
        (
            "remove-remote",
            |_| act(r#"{"action":"remove-remote","name":"origin"}"#),
            false,
        ),
        (
            "rename-remote",
            |_| act(r#"{"action":"rename-remote","from":"origin","to":"upstream"}"#),
            false,
        ),
        (
            "set-remote-url",
            |_| {
                act(
                    r#"{"action":"set-remote-url","name":"origin","url":"https://example.com/y.git"}"#,
                )
            },
            false,
        ),
    ];
    for (name, setup, backup) in cases {
        let f = scene(&format!("undo-{name}"));
        let action = setup(&f);
        let before = state(f.dir());
        let p = plan(&f.repo(), &action);
        assert!(p.blocked.is_none(), "{name}: {:?}", p.blocked);
        assert!(p.undoable, "{name}");
        assert_eq!(p.backup, backup, "{name}: backup");
        let r = apply(&f.repo(), &action);
        let entry = r
            .undo
            .clone()
            .unwrap_or_else(|| panic!("{name}: an undo entry"));
        if backup {
            assert!(!entry.backups.is_empty(), "{name}: backups");
        }
        for b in &entry.backups {
            assert!(b.starts_with("refs/tailor/backup/"), "{name}: {b}");
            assert!(has_ref(f.dir(), b), "{name}: {b} exists");
        }
        assert_ne!(
            state(f.dir()),
            before,
            "{name}: the action changed something"
        );
        let last = f.repo().last_action().unwrap().unwrap();
        assert_eq!(last.id, entry.id, "{name}");
        assert_eq!(last.stale, None, "{name}");
        let u = f
            .repo()
            .undo(&entry.id, false)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(u.message.starts_with("Undone: "), "{name}");
        assert_eq!(state(f.dir()), before, "{name}: undo restores everything");
        assert!(
            f.repo()
                .last_action()
                .unwrap()
                .is_none_or(|l| l.id != entry.id),
            "{name}"
        );
    }
}

#[test]
fn undo_takes_back_a_merge_that_stopped_on_conflicts() {
    let f = fixture("undo-conflict");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#),
    );
    f.commit("theirs", &[("notes.txt", "theirs\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    f.commit("ours", &[("notes.txt", "ours\n")]);
    let before = state(f.dir());
    let r = apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    assert!(f.repo().status().unwrap().merging);
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(state(f.dir()), before);
}

#[test]
fn undo_is_refused_once_the_branch_moved_since() {
    let f = fixture("undo-stale");
    let r = apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"x","start":"HEAD","switch":true}"#),
    );
    f.commit("later", &[("notes.txt", "later\n")]);
    let last = f.repo().last_action().unwrap().unwrap();
    let stale = last.stale.unwrap();
    assert!(stale.contains("changed since"), "{stale}");
    let err = f.repo().undo(&r.undo.unwrap().id, false).unwrap_err();
    assert!(err.to_string().contains("Cannot undo"), "{err}");
}

#[test]
fn undo_is_refused_while_the_app_has_unsaved_changes_or_files_were_edited() {
    let f = fixture("undo-edited");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#),
    );
    f.commit("other", &[("notes.txt", "other\n")]);
    let r = apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    let id = r.undo.unwrap().id;
    assert!(
        f.repo()
            .undo(&id, true)
            .unwrap_err()
            .to_string()
            .contains("Save your changes")
    );
    f.write("notes.txt", "edited since\n");
    let err = f.repo().undo(&id, false).unwrap_err();
    assert!(err.to_string().contains("notes.txt"), "{err}");
    assert_eq!(f.read("notes.txt"), "edited since\n", "nothing overwritten");
    assert_eq!(
        f.repo().status().unwrap().branch.as_deref(),
        Some("main"),
        "nothing moved"
    );
}

#[test]
fn network_actions_have_no_undo() {
    let f = fixture("undo-push");
    f.commit("ahead", &[("notes.txt", "ahead\n")]);
    for a in [r#"{"action":"push"}"#, r#"{"action":"fetch"}"#] {
        let p = plan(&f.repo(), &act(a));
        assert!(!p.undoable && p.network, "{a}");
        assert!(apply(&f.repo(), &act(a)).undo.is_none(), "{a}");
    }
    assert!(f.repo().last_action().unwrap().is_none());
}

#[test]
fn the_journal_keeps_the_newest_entries() {
    let f = fixture("undo-journal");
    let mut ids = Vec::new();
    for n in 0..UNDO_KEEP + 5 {
        let r = apply(
            &f.repo(),
            &act(&format!(
                r#"{{"action":"create-tag","name":"t{n}","target":"HEAD"}}"#
            )),
        );
        ids.push(r.undo.unwrap().id);
    }
    let text = std::fs::read_to_string(f.dir().join(".git/tailor/actions.jsonl")).unwrap();
    assert_eq!(text.lines().count(), UNDO_KEEP);
    assert!(
        f.repo()
            .undo(&ids[0], false)
            .unwrap_err()
            .to_string()
            .contains("no longer")
    );
    f.repo().undo(ids.last().unwrap(), false).unwrap();
    assert!(f.repo().rev_parse(&format!("t{}", UNDO_KEEP + 4)).is_err());
    assert!(f.repo().rev_parse("t0").is_ok());
}

#[test]
fn a_pull_is_undone_with_its_backup() {
    let f = fixture("undo-pull");
    let parent = tmp("undo-pull-clone");
    let other = tk_git::clone(
        &tk_git::local_url(f.remote.path()),
        parent.path(),
        &tk_git::NoCredentials,
        &opts(),
    )
    .unwrap();
    set_user(&other, "Bob");
    write(&other, "notes.txt", "bob\n");
    reopen(&other)
        .commit("bob", &paths(&["notes.txt"]), None)
        .unwrap();
    reopen(&other).push(&tk_git::NoCredentials).unwrap();
    let head = f.repo().rev_parse("HEAD").unwrap();
    let r = apply(&f.repo(), &act(r#"{"action":"pull"}"#));
    assert_eq!(f.read("notes.txt"), "bob\n");
    let entry = r.undo.unwrap();
    assert_eq!(ref_id(f.dir(), &entry.backups[0]), Some(head.clone()));
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(f.repo().rev_parse("HEAD").unwrap(), head);
    assert_eq!(f.read("notes.txt"), NOTES);
    assert!(f.repo().status().unwrap().files.is_empty());
}

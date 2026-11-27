// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Undo for every action (GitTailor wave 2, strand B, gate point B-2): every `GitAction`
//! variant plans `undoable = true` once it is not blocked (the network actions fetch, push,
//! publish and push-tag where the product opts in with `Repo::with_push_undo`), fetch and
//! push-tag are taken back, and an action plus its undo leaves the index as it was (staged
//! changes and conflict stages included), as `git ls-files -s` shows it.

mod common;
mod wizard;

use common::*;
use tk_git::Repo;
use tk_git_types::actions::GitAction;
use wizard::*;

/// The tag of every variant; the `match` fails to compile when a variant is added, so the
/// list below has to grow with it.
fn tag(a: &GitAction) -> &'static str {
    match a {
        GitAction::CreateBranch { .. } => "create-branch",
        GitAction::Switch { .. } => "switch",
        GitAction::RenameBranch { .. } => "rename-branch",
        GitAction::DeleteBranch { .. } => "delete-branch",
        GitAction::Merge { .. } => "merge",
        GitAction::Pull => "pull",
        GitAction::Push => "push",
        GitAction::Publish { .. } => "publish",
        GitAction::Fetch { .. } => "fetch",
        GitAction::AddRemote { .. } => "add-remote",
        GitAction::RemoveRemote { .. } => "remove-remote",
        GitAction::RenameRemote { .. } => "rename-remote",
        GitAction::SetRemoteUrl { .. } => "set-remote-url",
        GitAction::Stash { .. } => "stash",
        GitAction::Unstash { .. } => "unstash",
        GitAction::DropStash { .. } => "drop-stash",
        GitAction::Revert { .. } => "revert",
        GitAction::CherryPick { .. } => "cherry-pick",
        GitAction::Reset { .. } => "reset",
        GitAction::Discard { .. } => "discard",
        GitAction::CreateTag { .. } => "create-tag",
        GitAction::DeleteTag { .. } => "delete-tag",
        GitAction::PushTag { .. } => "push-tag",
        GitAction::Amend { .. } => "amend",
        GitAction::Abort => "abort",
        GitAction::RestoreBranch { .. } => "restore-branch",
        GitAction::RestoreFiles { .. } => "restore-files",
        GitAction::Rewrite { .. } => "rewrite",
        GitAction::Commit { .. } => "commit",
    }
}

const ALL: usize = 29;

/// `main` with two commits of its own (not pushed) and the tag `v1`, a branch `other` with one
/// commit of its own.
fn scene(name: &str) -> Fixture {
    let f = fixture(name);
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#),
    );
    f.commit("other", &[("project.yml", "manifest: 2.0.0\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    f.commit("main", &[("notes.txt", "main\n")]);
    f.commit("main 2", &[("notes.txt", "main 2\n")]);
    apply(
        &f.repo(),
        &act(r#"{"action":"create-tag","name":"v1","target":"HEAD","message":"One"}"#),
    );
    f
}

/// A second clone of the remote pushes a commit (what a colleague does).
fn someone_pushes(f: &Fixture, keep: &mut Vec<Tmp>) {
    let n = keep.len();
    let parent = tmp("undo-all-colleague");
    let other = tk_git::clone(
        &tk_git::local_url(f.remote.path()),
        parent.path(),
        &tk_git::NoCredentials,
        &opts(),
    )
    .unwrap();
    set_user(&other, "Bob");
    write(&other, "bob.txt", &format!("bob {n}\n"));
    reopen(&other)
        .commit("bob", &paths(&["bob.txt"]), None)
        .unwrap();
    reopen(&other).push(&tk_git::NoCredentials).unwrap();
    keep.push(parent);
}

fn repo(f: &Fixture) -> Repo {
    f.repo().with_push_undo(true)
}

/// A fetch with the product's opt-in.
fn fetch(f: &Fixture) -> tk_git_types::actions::GitActionResult {
    repo(f)
        .apply(&act(r#"{"action":"fetch"}"#), &options(false))
        .unwrap()
}

/// `git ls-files -s` (mode, id, stage, path of every index entry).
fn index(dir: &std::path::Path) -> String {
    git(dir, &["ls-files", "-s"]).unwrap_or_default()
}

type Setup = fn(&Fixture, &mut Vec<Tmp>) -> GitAction;

#[test]
fn b2_every_action_plans_undoable_once_it_is_not_blocked() {
    let cases: Vec<(&str, Setup)> = vec![
        ("create-branch", |_, _| {
            act(r#"{"action":"create-branch","name":"x","start":"HEAD"}"#)
        }),
        ("switch", |_, _| {
            act(r#"{"action":"switch","target":"other"}"#)
        }),
        ("rename-branch", |_, _| {
            act(r#"{"action":"rename-branch","from":"other","to":"renamed"}"#)
        }),
        ("delete-branch", |_, _| {
            act(r#"{"action":"delete-branch","name":"other","force":true}"#)
        }),
        ("merge", |_, _| act(r#"{"action":"merge","from":"other"}"#)),
        ("pull", |f, keep| {
            someone_pushes(f, keep);
            act(r#"{"action":"pull"}"#)
        }),
        ("push", |_, _| act(r#"{"action":"push"}"#)),
        ("publish", |_, _| {
            act(r#"{"action":"publish","branch":"other","remote":"origin"}"#)
        }),
        ("fetch", |f, keep| {
            someone_pushes(f, keep);
            act(r#"{"action":"fetch"}"#)
        }),
        ("add-remote", |_, _| {
            act(r#"{"action":"add-remote","name":"backup","url":"https://example.com/x.git"}"#)
        }),
        ("remove-remote", |_, _| {
            act(r#"{"action":"remove-remote","name":"origin"}"#)
        }),
        ("rename-remote", |_, _| {
            act(r#"{"action":"rename-remote","from":"origin","to":"upstream"}"#)
        }),
        ("set-remote-url", |_, _| {
            act(r#"{"action":"set-remote-url","name":"origin","url":"https://example.com/y.git"}"#)
        }),
        ("stash", |f, _| {
            f.write("notes.txt", "uncommitted\n");
            act(r#"{"action":"stash","message":"wip"}"#)
        }),
        ("unstash", |f, _| {
            f.write("notes.txt", "uncommitted\n");
            apply(&f.repo(), &act(r#"{"action":"stash","message":"wip"}"#));
            act(r#"{"action":"unstash","index":0}"#)
        }),
        ("drop-stash", |f, _| {
            f.write("notes.txt", "uncommitted\n");
            apply(&f.repo(), &act(r#"{"action":"stash","message":"wip"}"#));
            act(r#"{"action":"drop-stash","index":0}"#)
        }),
        ("revert", |_, _| {
            act(r#"{"action":"revert","commit":"HEAD"}"#)
        }),
        ("cherry-pick", |_, _| {
            act(r#"{"action":"cherry-pick","commit":"other"}"#)
        }),
        ("reset", |_, _| {
            act(r#"{"action":"reset","target":"HEAD~1","mode":"keep-changes"}"#)
        }),
        ("discard", |f, _| {
            f.write("notes.txt", "uncommitted\n");
            act(r#"{"action":"discard","paths":[]}"#)
        }),
        ("create-tag", |_, _| {
            act(r#"{"action":"create-tag","name":"v2","target":"HEAD~1"}"#)
        }),
        ("delete-tag", |_, _| {
            act(r#"{"action":"delete-tag","name":"v1"}"#)
        }),
        ("push-tag", |_, _| {
            act(r#"{"action":"push-tag","name":"v1","remote":"origin"}"#)
        }),
        ("amend", |_, _| {
            act(r#"{"action":"amend","message":"main 2, better"}"#)
        }),
        ("abort", |f, _| {
            apply(&f.repo(), &act(r#"{"action":"switch","target":"other"}"#));
            f.commit("theirs", &[("notes.txt", "theirs\n")]);
            apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
            apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
            act(r#"{"action":"abort"}"#)
        }),
        ("restore-branch", |_, _| {
            act(r#"{"action":"restore-branch","name":"other","target":"HEAD~1"}"#)
        }),
        ("restore-files", |_, _| {
            act(r#"{"action":"restore-files","rev":"HEAD~2","paths":["/notes.txt"]}"#)
        }),
        ("rewrite", |_, _| {
            act(r#"{"action":"rewrite","branch":"other","tip":"main","summary":"Squash"}"#)
        }),
        ("commit", |f, _| {
            f.write("notes.txt", "changed\n");
            act(r#"{"action":"commit","message":"Change","paths":["/notes.txt"]}"#)
        }),
    ];
    let mut seen: Vec<&str> = Vec::new();
    for (name, setup) in cases {
        let f = scene(&format!("undo-all-{name}"));
        let mut keep = Vec::new();
        let action = setup(&f, &mut keep);
        assert_eq!(tag(&action), name);
        seen.push(tag(&action));
        let p = repo(&f).plan(&action, false).unwrap();
        assert!(p.blocked.is_none(), "{name}: {:?}", p.blocked);
        assert!(p.undoable, "{name} plans undoable");
        let before = state(f.dir());
        let index_before = index(f.dir());
        let r = repo(&f)
            .apply(&action, &options(false))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let entry = r.undo.unwrap_or_else(|| panic!("{name}: an undo entry"));
        repo(&f)
            .undo(&entry.id, false)
            .unwrap_or_else(|e| panic!("{name}: undo: {e}"));
        assert_eq!(state(f.dir()), before, "{name}: undo restores everything");
        assert_eq!(index(f.dir()), index_before, "{name}: the index too");
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), ALL, "every variant has a case");
}

#[test]
fn b2_undo_of_a_fetch_puts_the_remote_tracking_refs_back() {
    let f = scene("undo-all-fetch");
    let mut keep = Vec::new();
    someone_pushes(&f, &mut keep);
    let before = ref_id(f.dir(), "refs/remotes/origin/main");
    assert!(!plan(&f.repo(), &act(r#"{"action":"fetch"}"#)).undoable);
    let r = fetch(&f);
    let after = ref_id(f.dir(), "refs/remotes/origin/main");
    assert_ne!(after, before);
    let entry = r.undo.expect("a fetch that brought something is undoable");
    assert!(
        entry
            .refs
            .iter()
            .any(|m| m.name == "refs/remotes/origin/main")
    );
    // A second fetch brings nothing: no entry, the first one stays the newest.
    assert!(fetch(&f).undo.is_none());
    assert_eq!(f.repo().last_action().unwrap().unwrap().id, entry.id);
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(ref_id(f.dir(), "refs/remotes/origin/main"), before);
}

#[test]
fn b2_undo_of_a_fetch_is_refused_once_another_fetch_moved_the_refs() {
    let f = scene("undo-all-fetch-stale");
    let mut keep = Vec::new();
    someone_pushes(&f, &mut keep);
    let entry = fetch(&f).undo.unwrap();
    someone_pushes(&f, &mut keep);
    apply(&f.repo(), &act(r#"{"action":"fetch"}"#));
    let err = f.repo().undo(&entry.id, false).unwrap_err();
    assert!(err.to_string().contains("changed since"), "{err}");
}

#[test]
fn b2_undo_of_a_pushed_tag_deletes_it_on_the_remote_with_a_lease() {
    for (name, annotated) in [("light", false), ("annotated", true)] {
        let f = scene(&format!("undo-all-push-tag-{name}"));
        let message = if annotated {
            r#","message":"Release""#
        } else {
            ""
        };
        apply(
            &f.repo(),
            &act(&format!(
                r#"{{"action":"create-tag","name":"{name}","target":"HEAD~1"{message}}}"#
            )),
        );
        let push = act(&format!(
            r#"{{"action":"push-tag","name":"{name}","remote":"origin"}}"#
        ));
        // Without the product's opt-in it stays a one-way action, as before.
        assert!(!plan(&f.repo(), &push).undoable, "{name}");
        let r = repo(&f).apply(&push, &options(false)).unwrap();
        let tag_ref = format!("refs/tags/{name}");
        assert!(ref_id(f.remote.path(), &tag_ref).is_some(), "{name} pushed");
        repo(&f).undo(&r.undo.unwrap().id, false).unwrap();
        assert_eq!(ref_id(f.remote.path(), &tag_ref), None, "{name} deleted");
        assert!(ref_id(f.dir(), &tag_ref).is_some(), "{name} kept locally");
    }
}

#[test]
fn b2_a_pushed_tag_that_changed_on_the_remote_is_not_deleted() {
    let f = scene("undo-all-push-tag-moved");
    let r = repo(&f)
        .apply(
            &act(r#"{"action":"push-tag","name":"v1","remote":"origin"}"#),
            &options(false),
        )
        .unwrap();
    // Someone replaces the tag on the server.
    let remote = f.remote.path().to_str().unwrap().to_owned();
    git(f.dir(), &["tag", "-f", "v1", "HEAD~1"]);
    git(f.dir(), &["push", "-q", "-f", &remote, "refs/tags/v1"]);
    let moved = ref_id(f.remote.path(), "refs/tags/v1");
    assert!(repo(&f).undo(&r.undo.unwrap().id, false).is_err());
    assert_eq!(ref_id(f.remote.path(), "refs/tags/v1"), moved);
}

#[test]
fn b2_staged_changes_survive_an_action_and_its_undo() {
    let f = scene("undo-all-staged");
    // Staged and unstaged changes on two files, one new file staged, one deletion staged.
    f.write("notes.txt", "staged\n");
    f.repo().stage(&paths(&["notes.txt"])).unwrap();
    f.write("notes.txt", "staged, then edited\n");
    f.write("added.txt", "added\n");
    f.repo().stage(&paths(&["added.txt"])).unwrap();
    std::fs::remove_file(f.dir().join("project.yml")).unwrap();
    f.repo().stage(&paths(&["project.yml"])).unwrap();
    f.write("users/get.yml", "edited, not staged\n");
    apply(
        &f.repo(),
        &act(r#"{"action":"create-branch","name":"z","start":"HEAD"}"#),
    );
    for a in [
        r#"{"action":"commit","message":"Staged","staged":true}"#,
        r#"{"action":"commit","message":"One file","paths":["/users/get.yml"]}"#,
        r#"{"action":"switch","target":"z"}"#,
        r#"{"action":"create-branch","name":"y","start":"HEAD","switch":true}"#,
        r#"{"action":"amend","message":"main 2, renamed"}"#,
    ] {
        let action = act(a);
        let p = plan(&f.repo(), &action);
        if p.blocked.is_some() {
            panic!("{a}: {:?}", p.blocked);
        }
        let before = state(f.dir());
        let index_before = index(f.dir());
        let r = apply(&f.repo(), &action);
        f.repo().undo(&r.undo.unwrap().id, false).unwrap();
        assert_eq!(index(f.dir()), index_before, "{a}: index");
        assert_eq!(state(f.dir()), before, "{a}");
    }
}

#[test]
fn b2_conflict_stages_survive_an_abort_and_its_undo() {
    let f = scene("undo-all-conflict-stages");
    apply(&f.repo(), &act(r#"{"action":"switch","target":"other"}"#));
    f.commit("theirs", &[("notes.txt", "theirs\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    let staged = index(f.dir());
    assert!(staged.contains(" 3\tnotes.txt"), "{staged}");
    let r = apply(&f.repo(), &act(r#"{"action":"abort"}"#));
    assert!(!index(f.dir()).contains(" 3\t"));
    f.repo().undo(&r.undo.unwrap().id, false).unwrap();
    assert_eq!(index(f.dir()), staged);
}

// ---- recorded changes and the redo ----------------------------------------------------------

#[test]
fn b2_staging_is_recorded_and_undone_and_refused_once_the_index_moved() {
    let f = scene("undo-all-record-stage");
    f.write("notes.txt", "staged\n");
    f.write("new.txt", "new\n");
    let before = index(f.dir());
    let (_, entry) = f
        .repo()
        .record("stage", "Staged 2 files.", false, |r| {
            r.stage(&paths(&["notes.txt", "new.txt"]))
        })
        .unwrap();
    let entry = entry.expect("staging changed the index");
    assert_eq!(entry.action, "stage");
    assert_ne!(index(f.dir()), before);
    // Staging again changes nothing: no entry.
    let (_, none) = f
        .repo()
        .record("stage", "Staged.", false, |r| {
            r.stage(&paths(&["notes.txt"]))
        })
        .unwrap();
    assert!(none.is_none());
    assert_eq!(f.repo().action(&entry.id).unwrap().unwrap().stale, None);
    f.repo().undo(&entry.id, false).unwrap();
    assert_eq!(index(f.dir()), before);
    assert_eq!(f.read("notes.txt"), "staged\n", "the files stay");

    // Staged more since: refused, nothing changes.
    let (_, entry) = f
        .repo()
        .record("stage", "Staged.", false, |r| {
            r.stage(&paths(&["notes.txt"]))
        })
        .unwrap();
    f.repo().stage(&paths(&["new.txt"])).unwrap();
    let later = index(f.dir());
    let id = entry.unwrap().id;
    let stale = f.repo().action(&id).unwrap().unwrap().stale.unwrap();
    assert!(stale.contains("staged changes changed since"), "{stale}");
    assert!(f.repo().undo(&id, false).is_err());
    assert_eq!(index(f.dir()), later);
}

#[test]
fn b2_a_resolved_conflict_comes_back_with_its_stages() {
    let f = scene("undo-all-record-resolve");
    apply(&f.repo(), &act(r#"{"action":"switch","target":"other"}"#));
    f.commit("theirs", &[("notes.txt", "theirs\n")]);
    apply(&f.repo(), &act(r#"{"action":"switch","target":"main"}"#));
    apply(&f.repo(), &act(r#"{"action":"merge","from":"other"}"#));
    let before = state(f.dir());
    let staged = index(f.dir());
    let (_, entry) = f
        .repo()
        .record("resolve", "Took ours.", true, |r| {
            r.resolve("notes.txt", tk_git_types::GitResolution::Ours)
        })
        .unwrap();
    assert!(!index(f.dir()).contains(" 3\t"));
    f.repo().undo(&entry.unwrap().id, false).unwrap();
    assert_eq!(index(f.dir()), staged);
    assert_eq!(state(f.dir()), before);
}

#[test]
fn b2_a_recorded_file_change_is_stale_once_the_file_was_edited() {
    let f = scene("undo-all-record-file");
    let (_, entry) = f
        .repo()
        .record("restore", "Wrote notes.", true, |r| {
            std::fs::write(r.workdir().join("notes.txt"), "written\n")
                .map_err(|e| tk_git::GitError::Io(e.to_string()))
        })
        .unwrap();
    let id = entry.unwrap().id;
    f.write("notes.txt", "edited by hand\n");
    let stale = f.repo().action(&id).unwrap().unwrap().stale.unwrap();
    assert!(stale.contains("notes.txt"), "{stale}");
    assert!(f.repo().undo(&id, false).is_err());
    assert_eq!(f.read("notes.txt"), "edited by hand\n");
}

#[test]
fn b2_the_undo_of_an_undo_is_the_redo() {
    let f = scene("undo-all-redo");
    let mut keep = Vec::new();
    someone_pushes(&f, &mut keep);
    f.write("notes.txt", "staged\n");
    f.repo().stage(&paths(&["notes.txt"])).unwrap();
    for a in [
        r#"{"action":"commit","message":"Staged","staged":true}"#,
        r#"{"action":"switch","target":"other"}"#,
        r#"{"action":"merge","from":"other"}"#,
        r#"{"action":"pull"}"#,
    ] {
        let action = act(a);
        if plan(&f.repo(), &action).blocked.is_some() {
            continue;
        }
        let before = (state(f.dir()), index(f.dir()));
        let r = repo(&f).apply(&action, &options(false)).unwrap();
        let after = (state(f.dir()), index(f.dir()));
        let undo = repo(&f)
            .undo_recorded(&r.undo.unwrap().id, false, &tk_git::NoCredentials)
            .unwrap();
        assert_eq!((state(f.dir()), index(f.dir())), before, "{a}: undone");
        let undo = undo
            .undo
            .unwrap_or_else(|| panic!("{a}: the undo is journaled"));
        assert_eq!(undo.action, "undo");
        let redo = repo(&f)
            .undo_recorded(&undo.id, false, &tk_git::NoCredentials)
            .unwrap();
        assert_eq!((state(f.dir()), index(f.dir())), after, "{a}: redone");
        // And the redo can be undone again.
        repo(&f)
            .undo_recorded(&redo.undo.unwrap().id, false, &tk_git::NoCredentials)
            .unwrap();
        assert_eq!(
            (state(f.dir()), index(f.dir())),
            before,
            "{a}: undone again"
        );
        // Leave the repository after the action for the next one.
        repo(&f).apply(&action, &options(false)).unwrap();
    }
}

#[test]
fn b2_the_redo_of_an_undone_push_sends_it_again_with_a_lease() {
    for publish in [false, true] {
        let f = scene(&format!("undo-all-redo-push-{publish}"));
        let (action, remote_ref) = if publish {
            (
                act(r#"{"action":"publish","branch":"other","remote":"origin"}"#),
                "refs/heads/other",
            )
        } else {
            (act(r#"{"action":"push"}"#), "refs/heads/main")
        };
        let before = ref_id(f.remote.path(), remote_ref);
        let r = repo(&f).apply(&action, &options(false)).unwrap();
        let pushed = ref_id(f.remote.path(), remote_ref);
        assert_ne!(pushed, before);
        let undo = repo(&f)
            .undo_recorded(&r.undo.unwrap().id, false, &tk_git::NoCredentials)
            .unwrap();
        assert_eq!(ref_id(f.remote.path(), remote_ref), before, "{publish}");
        let undo_id = undo.undo.unwrap().id;
        assert!(repo(&f).pushed_ref(&undo_id).unwrap().is_some());
        repo(&f)
            .undo_recorded(&undo_id, false, &tk_git::NoCredentials)
            .unwrap();
        assert_eq!(ref_id(f.remote.path(), remote_ref), pushed, "{publish}");
    }
}

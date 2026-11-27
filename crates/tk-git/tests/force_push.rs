// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! GitTailor wave 3, strand B: `GitAction::ForcePush` is a lease (B-1), with
//! `--force-if-includes` (B-2), a backup of the overwritten remote tip and an undo with a lease
//! (B-3), and it is refused on protected branches and without `Repo::with_force_push` (B-4).
//! Every case runs twice from the same state: once through the kit, once through real `git push
//! --force-with-lease` on a copy of the folders (when `git` is installed); both must accept or
//! refuse alike and leave the remote at the same commit.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::*;
use tk_git::{ActionOptions, NoCredentials, Repo};
use tk_git_types::actions::{GitAction, GitActionResult, GitBlockReason, GitPlan, GitResetMode};

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn options() -> ActionOptions<'static> {
    ActionOptions {
        unsaved: false,
        credentials: &NoCredentials,
        identity: None,
    }
}

/// A folder with a bare remote `remote.git` and a working tree `a` whose `origin` is
/// `../remote.git` (relative, so a copy of the folder is a world of its own).
struct World {
    root: Tmp,
}

impl World {
    fn a(&self) -> PathBuf {
        self.root.path().join("a")
    }
    fn remote(&self) -> PathBuf {
        self.root.path().join("remote.git")
    }
    /// The working tree with force push allowed and push undo on.
    fn repo(&self) -> Repo {
        reopen(&self.a()).with_force_push(true).with_push_undo(true)
    }
    fn commit(&self, message: &str, file: &str, text: &str) -> String {
        write(&self.a(), file, text);
        reopen(&self.a())
            .commit(message, &paths(&[file]), None)
            .unwrap()
            .oid
    }
    fn remote_main(&self) -> Option<String> {
        ref_id(&self.remote(), "refs/heads/main")
    }
    fn tracking(&self) -> Option<String> {
        ref_id(&self.a(), "refs/remotes/origin/main")
    }
}

/// `main` with two commits, pushed; then rewritten locally: the second commit is replaced by
/// another one, so `main` and `origin/main` went apart and only a force push sends it.
/// Returns (world, the pushed tip `c2`, the rewritten tip `c3`).
fn rewritten(name: &str) -> (World, String, String) {
    let root = tmp(name);
    let w = World { root };
    bare(&w.remote());
    std::fs::create_dir_all(w.a()).unwrap();
    new_repo(&w.a(), "Ada");
    add_remote(&w.a(), "../remote.git");
    w.commit("one", "a.txt", "one\n");
    let c2 = w.commit("two", "a.txt", "two\n");
    reopen(&w.a()).push(&NoCredentials).unwrap();
    assert_eq!(w.remote_main().as_deref(), Some(c2.as_str()));
    apply(
        &w.repo(),
        &GitAction::Reset {
            target: "HEAD~1".into(),
            mode: GitResetMode::Discard,
        },
    );
    let c3 = w.commit("two, better", "a.txt", "two, better\n");
    (w, c2, c3)
}

fn apply(repo: &Repo, action: &GitAction) -> GitActionResult {
    repo.apply(action, &options())
        .unwrap_or_else(|e| panic!("{action:?}: {e}"))
}

fn plan(repo: &Repo, action: &GitAction) -> GitPlan {
    repo.plan(action, false).unwrap()
}

fn force(branch: Option<&str>, expect: &str) -> GitAction {
    GitAction::ForcePush {
        branch: branch.map(str::to_owned),
        expect: expect.to_owned(),
    }
}

/// Someone else pushes `file` on top of the remote's `main` (another clone).
fn someone_pushes(w: &World, file: &str) -> String {
    let other = w.root.path().join("other");
    let _ = std::fs::remove_dir_all(&other);
    let cloned = tk_git::clone(
        &w.remote().to_string_lossy(),
        w.root.path(),
        &NoCredentials,
        &opts(),
    )
    .unwrap();
    std::fs::rename(&cloned, &other).unwrap();
    set_user(&other, "Bob");
    write(&other, file, "theirs\n");
    let repo = reopen(&other);
    let id = repo.commit("theirs", &paths(&[file]), None).unwrap().oid;
    repo.push(&NoCredentials).unwrap();
    std::fs::remove_dir_all(&other).unwrap();
    id
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Real `git push <args> origin <branch>` in `a` of a copy of the world: whether it was
/// accepted, and the copy (for the remote's state after it).
fn git_push(w: &World, args: &[&str], branch: &str) -> (bool, Tmp) {
    let copy = tmp("force-push-git");
    copy_dir(w.root.path(), copy.path());
    let out = Command::new("git")
        .arg("-C")
        .arg(copy.path().join("a"))
        .arg("push")
        .args(args)
        .args(["origin", branch])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    (out.status.success(), copy)
}

/// The kit's force push of `branch` with `expect`: whether it went through.
fn kit_push(w: &World, branch: Option<&str>, expect: &str) -> bool {
    w.repo().apply(&force(branch, expect), &options()).is_ok()
}

/// B-1, B-2: the kit and `git` decide alike and leave the remote alike.
fn same_as_git(w: &World, branch: &str, expect: &str, git_args: &[&str], accepted: bool) {
    let remote_ref = format!("refs/heads/{branch}");
    let before = ref_id(&w.remote(), &remote_ref);
    let witness = have_git().then(|| git_push(w, git_args, branch));
    let kit = kit_push(w, Some(branch), expect);
    assert_eq!(
        kit,
        accepted,
        "the kit {}",
        if kit { "accepted" } else { "refused" }
    );
    let after = ref_id(&w.remote(), &remote_ref);
    if !accepted {
        assert_eq!(
            after, before,
            "a refused force push leaves the remote as it was"
        );
    }
    if let Some((git, copy)) = witness {
        assert_eq!(
            git,
            accepted,
            "git {}",
            if git { "accepted" } else { "refused" }
        );
        assert_eq!(
            ref_id(&copy.path().join("remote.git"), &remote_ref),
            after,
            "the remote after git and after the kit"
        );
    }
}

#[test]
fn unchanged_remote_is_replaced() {
    let (w, c2, c3) = rewritten("fp-unchanged");
    let lease = format!("--force-with-lease=refs/heads/main:{c2}");
    same_as_git(&w, "main", &c2, &[&lease], true);
    assert_eq!(w.remote_main(), Some(c3.clone()));
    assert_eq!(w.tracking(), Some(c3));
}

#[test]
fn moved_remote_is_refused() {
    let (w, c2, _) = rewritten("fp-moved");
    let theirs = someone_pushes(&w, "b.txt");
    // No fetch: the local tracking ref still shows c2, the remote has their commit.
    assert_eq!(w.tracking(), Some(c2.clone()));
    let p = plan(&w.repo(), &force(None, &c2));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    let lease = format!("--force-with-lease=refs/heads/main:{c2}");
    same_as_git(&w, "main", &c2, &[&lease], false);
    assert_eq!(w.remote_main(), Some(theirs));
    let err = w.repo().apply(&force(None, &c2), &options()).unwrap_err();
    assert!(err.to_string().contains("someone pushed"), "{err}");
}

#[test]
fn deleted_remote_is_refused() {
    let (w, c2, _) = rewritten("fp-deleted");
    let remote = gix::open(w.remote()).unwrap();
    remote
        .find_reference("refs/heads/main")
        .unwrap()
        .delete()
        .unwrap();
    let lease = format!("--force-with-lease=refs/heads/main:{c2}");
    same_as_git(&w, "main", &c2, &[&lease], false);
    assert_eq!(w.remote_main(), None);
}

#[test]
fn new_branch_needs_it_absent_on_the_remote() {
    let (w, _, c3) = rewritten("fp-new");
    // `fresh` has an upstream that does not exist on the remote yet.
    apply(
        &w.repo(),
        &GitAction::CreateBranch {
            name: "fresh".into(),
            start: "HEAD".into(),
            switch: false,
        },
    );
    append_config(
        &w.a(),
        "[branch \"fresh\"]\n\tremote = origin\n\tmerge = refs/heads/fresh\n",
    );
    let p = plan(&w.repo(), &force(Some("fresh"), ""));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(!p.backup, "nothing on the remote to keep");
    same_as_git(
        &w,
        "fresh",
        "",
        &["--force-with-lease=refs/heads/fresh:"],
        true,
    );
    assert_eq!(ref_id(&w.remote(), "refs/heads/fresh"), Some(c3));
}

#[test]
fn new_branch_that_exists_on_the_remote_is_refused() {
    let (w, c2, _) = rewritten("fp-new-exists");
    apply(
        &w.repo(),
        &GitAction::CreateBranch {
            name: "fresh".into(),
            start: "HEAD".into(),
            switch: false,
        },
    );
    append_config(
        &w.a(),
        "[branch \"fresh\"]\n\tremote = origin\n\tmerge = refs/heads/fresh\n",
    );
    // Someone published `fresh` meanwhile (at c2).
    let remote = gix::open(w.remote()).unwrap();
    remote
        .reference(
            "refs/heads/fresh",
            gix::ObjectId::from_hex(c2.as_bytes()).unwrap(),
            gix::refs::transaction::PreviousValue::MustNotExist,
            "test",
        )
        .unwrap();
    same_as_git(
        &w,
        "fresh",
        "",
        &["--force-with-lease=refs/heads/fresh:"],
        false,
    );
    assert_eq!(ref_id(&w.remote(), "refs/heads/fresh"), Some(c2));
}

#[test]
fn a_fetched_commit_never_seen_here_is_not_overwritten() {
    // B-2: the background fetch brought their commit into origin/main; the plan offers it as
    // the lease, but it was never integrated locally (`--force-if-includes`).
    let (w, _, _) = rewritten("fp-includes");
    let theirs = someone_pushes(&w, "b.txt");
    apply(&w.repo(), &GitAction::Fetch { remote: None });
    assert_eq!(w.tracking(), Some(theirs.clone()));
    let p = plan(&w.repo(), &force(None, &theirs));
    let b = p.blocked.expect("blocked");
    assert_eq!(b.reason, GitBlockReason::Rejected);
    assert!(b.message.contains("Someone pushed"), "{}", b.message);
    same_as_git(
        &w,
        "main",
        &theirs,
        &["--force-with-lease", "--force-if-includes"],
        false,
    );
    assert_eq!(w.remote_main(), Some(theirs));
}

#[test]
fn a_fetched_commit_integrated_once_is_overwritten() {
    // B-2, the other way: their commit was in the branch once (reflog), then left out again.
    let (w, _, c3) = rewritten("fp-included");
    let theirs = someone_pushes(&w, "b.txt");
    apply(&w.repo(), &GitAction::Fetch { remote: None });
    for target in [theirs.as_str(), c3.as_str()] {
        apply(
            &w.repo(),
            &GitAction::Reset {
                target: target.into(),
                mode: GitResetMode::Discard,
            },
        );
    }
    assert!(w.repo().integrated("main", &theirs).unwrap());
    same_as_git(
        &w,
        "main",
        &theirs,
        &["--force-with-lease", "--force-if-includes"],
        true,
    );
    assert_eq!(w.remote_main(), Some(c3));
}

#[test]
fn the_plan_lists_what_the_remote_loses() {
    let (w, c2, c3) = rewritten("fp-plan");
    let p = plan(&w.repo(), &force(None, &c2));
    assert!(p.blocked.is_none(), "{:?}", p.blocked);
    assert!(p.network && p.backup && p.undoable);
    assert_eq!(p.refs.len(), 1);
    assert_eq!(p.refs[0].name, "refs/remotes/origin/main");
    assert_eq!(p.refs[0].before.as_deref(), Some(c2.as_str()));
    assert_eq!(p.refs[0].after.as_deref(), Some(c3.as_str()));
    assert!(p.steps[0].contains("1 commit"), "{:?}", p.steps);
    let lost = w.repo().overwritten_by(&c3, &c2).unwrap();
    assert_eq!(lost.len(), 1);
    assert_eq!(lost[0].oid, c2);
    let t = w.repo().force_push_target(None).unwrap().unwrap();
    assert_eq!(
        (t.branch.as_str(), t.remote.as_str(), t.remote_ref.as_str()),
        ("main", "origin", "refs/heads/main")
    );
    assert_eq!(t.tracked, Some(c2.clone()));
    // Nothing to do when the remote already has it.
    let p = plan(&w.repo(), &force(None, &c3));
    assert_eq!(p.blocked.unwrap().reason, GitBlockReason::NothingToDo);
    // A lease the tracking ref does not show any more: a warning, the remote decides.
    let one = w.repo().force_push_target(None).unwrap().unwrap().tracked;
    assert_eq!(one, Some(c2));
}

#[test]
fn force_push_keeps_a_backup_and_undo_puts_the_remote_back_with_a_lease() {
    // B-3 (kit part).
    let (w, c2, c3) = rewritten("fp-undo");
    let done = apply(&w.repo(), &force(None, &c2));
    assert_eq!(w.remote_main(), Some(c3.clone()));
    let backups: Vec<String> = gix::open(w.a())
        .unwrap()
        .references()
        .unwrap()
        .prefixed("refs/tailor/backup/")
        .unwrap()
        .map(|r| r.unwrap().name().as_bstr().to_string())
        .filter(|n| n.ends_with("/remote-origin-main"))
        .collect();
    assert_eq!(backups.len(), 1, "one backup of the overwritten tip");
    assert_eq!(ref_id(&w.a(), &backups[0]), Some(c2.clone()));
    let entry = done.undo.expect("journal entry");
    assert_eq!(entry.action, "force-push");
    w.repo()
        .undo_with(&entry.id, false, &NoCredentials)
        .unwrap();
    assert_eq!(w.remote_main(), Some(c2.clone()), "the remote is back");
    assert_eq!(w.tracking(), Some(c2.clone()));
    assert_eq!(ref_id(&w.a(), "refs/heads/main"), Some(c3), "local stays");
    // The tip undo put back was made here: a new force push is not refused as someone else's.
    let again = plan(&w.repo(), &force(None, &c2));
    assert!(again.blocked.is_none(), "{:?}", again.blocked);
}

#[test]
fn undo_is_refused_once_someone_pushed_on_top() {
    let (w, c2, _) = rewritten("fp-undo-stale");
    let done = apply(&w.repo(), &force(None, &c2));
    let theirs = someone_pushes(&w, "b.txt");
    let entry = done.undo.expect("journal entry");
    let err = w
        .repo()
        .undo_with(&entry.id, false, &NoCredentials)
        .unwrap_err();
    assert!(err.to_string().contains("Cannot undo the push"), "{err}");
    assert_eq!(w.remote_main(), Some(theirs), "their work stays");
}

#[test]
fn a_refused_force_push_leaves_no_backup() {
    let (w, c2, _) = rewritten("fp-no-backup");
    someone_pushes(&w, "b.txt");
    assert!(!kit_push(&w, None, &c2));
    let any = gix::open(w.a())
        .unwrap()
        .references()
        .unwrap()
        .prefixed("refs/tailor/backup/")
        .unwrap()
        .filter_map(|r| r.ok())
        .any(|r| r.name().as_bstr().to_string().contains("remote-"));
    assert!(!any);
    assert!(
        w.repo()
            .last_action()
            .unwrap()
            .is_none_or(|e| e.action != "force-push")
    );
}

#[test]
fn protected_branches_and_the_opt_in_block_it() {
    // B-4 (kit part).
    let (w, c2, _) = rewritten("fp-protected");
    let plain = reopen(&w.a());
    let p = plan(&plain, &force(None, &c2));
    assert_eq!(p.blocked.unwrap().reason, GitBlockReason::Unsupported);
    assert!(plain.apply(&force(None, &c2), &options()).is_err());

    let guarded = w
        .repo()
        .with_protected(vec!["main".into(), "release/*".into()]);
    let p = plan(&guarded, &force(None, &c2));
    let b = p.blocked.unwrap();
    assert_eq!(b.reason, GitBlockReason::Protected);
    assert!(b.message.contains("main"), "{}", b.message);
    assert!(guarded.apply(&force(None, &c2), &options()).is_err());
    assert_eq!(w.remote_main(), Some(c2));
    assert_eq!(
        guarded.protected_pattern("release/2.0").as_deref(),
        Some("release/*")
    );
    assert_eq!(guarded.protected_pattern("release/2/x"), None);
    assert_eq!(guarded.protected_pattern("feature"), None);

    let elsewhere = w.repo().with_protected(vec!["release/*".into()]);
    assert!(
        plan(
            &elsewhere,
            &force(None, &ref_id(&w.remote(), "refs/heads/main").unwrap())
        )
        .blocked
        .is_none()
    );
}

#[test]
fn a_plain_push_of_a_rewritten_branch_stays_refused() {
    // Push stays fast-forward only, also where force push is allowed.
    let (w, c2, _) = rewritten("fp-plain");
    let p = plan(&w.repo(), &GitAction::Push);
    assert_eq!(p.blocked.unwrap().reason, GitBlockReason::Diverged);
    assert!(w.repo().push(&NoCredentials).is_err());
    assert_eq!(w.remote_main(), Some(c2));
}

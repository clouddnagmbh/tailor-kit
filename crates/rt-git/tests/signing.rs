// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The signature hooks (wave 15, strand F): with a test `Signing`, every way the app writes
//! a commit (commit, commit-staged, merge, cherry-pick, revert, amend, `write_commit`,
//! commit-to-branch) stores the signer's armor in `gpgsig` over exactly the payload Git
//! would sign; the graph and `commit_info` report `signed`; `verify` asks the verifier, or
//! says `unverified`/`unsigned` without one. Without a `Signing` nothing changes.

mod common;
mod wizard;

use std::sync::{Arc, Mutex};

use common::*;
use rt_app::AppError;
use rt_app::git::graph::GitGraphQuery;
use rt_app::git::index::GitStagedCommit;
use rt_app::git::plumbing::GitCommitWrite;
use rt_app::git::signing::{GitSignatureCheck, GitSignatureKind, GitSignatureState};
use rt_git::TreeFile;
use rt_git::sign::{CommitSigner, SignatureVerifier, Signing};
use wizard::*;

/// "Signs" with a checksum of the payload; remembers every payload it was given.
#[derive(Default)]
struct TestSigner {
    payloads: Mutex<Vec<Vec<u8>>>,
    refuse: bool,
    skip: bool,
}

fn armor(payload: &[u8]) -> String {
    let sum = payload
        .iter()
        .fold(0u64, |a, b| a.wrapping_mul(131).wrapping_add(u64::from(*b)));
    // A blank line inside the armor, as real ones have, exercises the header folding.
    format!(
        "-----BEGIN SSH SIGNATURE-----\n{sum:016x}\n\nEND-OF-BODY\n-----END SSH SIGNATURE-----\n"
    )
}

impl CommitSigner for TestSigner {
    fn sign(&self, _workdir: &std::path::Path, payload: &[u8]) -> Result<Option<String>, AppError> {
        if self.refuse {
            return Err(AppError::Git("the signing key is locked".into()));
        }
        self.payloads.lock().unwrap().push(payload.to_vec());
        Ok((!self.skip).then(|| armor(payload)))
    }
}

struct TestVerifier;

impl SignatureVerifier for TestVerifier {
    fn verify(
        &self,
        _workdir: &std::path::Path,
        oid: &str,
        payload: &[u8],
        signature: &str,
    ) -> Result<GitSignatureCheck, AppError> {
        let good = signature == armor(payload);
        Ok(GitSignatureCheck {
            oid: oid.to_owned(),
            state: if good {
                GitSignatureState::Good
            } else {
                GitSignatureState::Bad
            },
            kind: Some(GitSignatureKind::Ssh),
            signer: good.then(|| "ada@example.com".to_owned()),
            key: None,
        })
    }
}

fn signing(signer: Arc<TestSigner>, verify: bool) -> Signing {
    Signing {
        signer: Some(signer),
        verifier: verify.then(|| Arc::new(TestVerifier) as Arc<dyn SignatureVerifier>),
    }
}

/// The commit object as Git stores it.
fn raw(dir: &std::path::Path, rev: &str) -> String {
    git(dir, &["cat-file", "commit", rev]).unwrap()
}

/// Raw commit without its `gpgsig` header (what is signed).
fn unsigned_payload(raw: &str) -> String {
    let (head, msg) = raw.split_once("\n\n").unwrap();
    let mut out = String::new();
    let mut skipping = false;
    for line in head.lines() {
        if line.starts_with("gpgsig ") {
            skipping = true;
        } else if skipping && line.starts_with(' ') {
        } else {
            skipping = false;
            out.push_str(line);
            out.push('\n');
        }
    }
    format!("{out}\n{msg}")
}

fn assert_signed(dir: &std::path::Path, rev: &str, signer: &TestSigner) {
    let raw = raw(dir, rev);
    assert!(
        raw.contains("\ngpgsig -----BEGIN SSH SIGNATURE-----\n"),
        "{raw}"
    );
    assert!(raw.contains("\n -----END SSH SIGNATURE-----\n"), "{raw}");
    // Git itself reads the folded header (and fsck is happy with the object).
    assert!(git(dir, &["fsck", "--no-progress"]).is_some());
    let payload = unsigned_payload(&raw);
    let seen = signer.payloads.lock().unwrap();
    assert!(
        seen.iter().any(|p| p.as_slice() == payload.as_bytes()),
        "the signer was not given the commit without gpgsig:\n{payload}"
    );
}

fn signed_fixture(name: &str) -> (Fixture, Arc<TestSigner>) {
    let f = fixture(name);
    let signer = Arc::new(TestSigner::default());
    (f, signer)
}

fn repo_with(f: &Fixture, signer: &Arc<TestSigner>) -> rt_git::Repo {
    f.repo().with_signing(signing(signer.clone(), true))
}

#[test]
fn commit_and_commit_staged_are_signed() {
    let (f, signer) = signed_fixture("sign-commit");
    f.write("notes.txt", "a\n");
    let repo = repo_with(&f, &signer);
    let c = repo.commit("signed", &paths(&["notes.txt"]), None).unwrap();
    assert_signed(f.dir(), &c.oid, &signer);
    assert_eq!(git(f.dir(), &["rev-parse", "HEAD"]).unwrap().trim(), c.oid);
    // The reflog line is the one `git commit` writes.
    let log = git(f.dir(), &["reflog", "-1", "--format=%gs"]).unwrap();
    assert_eq!(log.trim(), "commit: signed");

    f.write("notes.txt", "b\n");
    repo.stage(&paths(&["/notes.txt"])).unwrap();
    let c = repo
        .commit_staged(
            &GitStagedCommit {
                message: "staged".into(),
            },
            None,
        )
        .unwrap();
    assert_signed(f.dir(), &c.oid, &signer);

    // The info and the verdicts.
    let info = repo.commit_info(&c.oid).unwrap();
    assert!(info.signed);
    let checks = repo
        .verify(&[c.oid.clone(), format!("{}~2", c.oid)])
        .unwrap();
    assert_eq!(checks[0].state, GitSignatureState::Good);
    assert_eq!(checks[0].kind, Some(GitSignatureKind::Ssh));
    assert_eq!(checks[0].signer.as_deref(), Some("ada@example.com"));
    assert_eq!(checks[1].state, GitSignatureState::Unsigned);
}

#[test]
fn merge_cherry_pick_revert_and_amend_are_signed() {
    let (f, signer) = signed_fixture("sign-actions");
    let repo = repo_with(&f, &signer);
    let go = |json: &str| {
        repo.apply(&act(json), &options(false)).unwrap();
    };
    go(r#"{"action":"create-branch","name":"other","start":"HEAD","switch":true}"#);
    f.commit("on other", &[("other.txt", "o\n")]);
    go(r#"{"action":"switch","target":"main"}"#);
    f.commit("on main", &[("main.txt", "m\n")]);
    go(r#"{"action":"create-branch","name":"third","start":"main~1","switch":true}"#);
    let on_third = f.commit("on third", &[("third.txt", "t\n")]);
    go(r#"{"action":"switch","target":"main"}"#);

    // Merge (a real merge commit with two parents).
    go(r#"{"action":"merge","from":"other"}"#);
    assert_signed(f.dir(), "HEAD", &signer);
    assert_eq!(
        git(f.dir(), &["rev-list", "--parents", "-n", "1", "HEAD"])
            .unwrap()
            .split_whitespace()
            .count(),
        3
    );

    // Revert a commit, then copy the commit of the third branch.
    f.write("later.txt", "l\n");
    repo.commit("later", &paths(&["later.txt"]), None).unwrap();
    let later = git(f.dir(), &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_owned();
    go(&format!(r#"{{"action":"revert","commit":"{later}"}}"#));
    assert_signed(f.dir(), "HEAD", &signer);
    go(&format!(
        r#"{{"action":"cherry-pick","commit":"{on_third}"}}"#
    ));
    assert_signed(f.dir(), "HEAD", &signer);
    // The picked commit keeps its author and message.
    assert_eq!(
        git(f.dir(), &["log", "-1", "--format=%s"]).unwrap().trim(),
        "on third"
    );

    // Amend.
    go(r#"{"action":"amend","message":"reworded"}"#);
    assert_signed(f.dir(), "HEAD", &signer);
    assert_eq!(
        git(f.dir(), &["log", "-1", "--format=%s"]).unwrap().trim(),
        "reworded"
    );
    assert!(!raw(f.dir(), "HEAD").contains("gpgsig gpgsig"));
}

#[test]
fn write_commit_and_commit_to_branch_are_signed_and_move_no_ref_for_the_first() {
    let (f, signer) = signed_fixture("sign-plumbing");
    let repo = repo_with(&f, &signer);
    let head = git(f.dir(), &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_owned();
    let tree = git(f.dir(), &["rev-parse", "HEAD^{tree}"])
        .unwrap()
        .trim()
        .to_owned();
    let before = raw_state(f.dir());
    let oid = repo
        .write_commit(
            &GitCommitWrite {
                tree,
                parents: vec![head.clone()],
                message: "built by hand\n".into(),
                author: None,
                committer: None,
            },
            None,
        )
        .unwrap();
    assert_signed(f.dir(), &oid, &signer);
    assert_eq!(raw_state(f.dir()), before, "no ref moved");

    let c = repo
        .commit_to_branch(
            "recordings",
            "HEAD",
            &[TreeFile::write("rec/a.txt", "x\n")],
            "record",
            None,
        )
        .unwrap();
    assert_signed(f.dir(), &c.oid, &signer);
    assert_eq!(ref_id(f.dir(), "refs/heads/recordings").unwrap(), c.oid);
    // The branch was created (as before) and the checked-out one did not move.
    assert_eq!(git(f.dir(), &["rev-parse", "HEAD"]).unwrap().trim(), head);
}

#[test]
fn the_graph_shows_signed_and_verify_answers_without_a_verifier() {
    let (f, signer) = signed_fixture("sign-graph");
    let repo = f.repo().with_signing(Signing {
        signer: Some(signer.clone()),
        verifier: None,
    });
    f.write("notes.txt", "a\n");
    let c = repo.commit("signed", &paths(&["notes.txt"]), None).unwrap();
    let graph = repo
        .commit_graph(&GitGraphQuery {
            refs: None,
            skip: 0,
            limit: 10,
        })
        .unwrap();
    let signed: Vec<(&str, bool)> = graph
        .commits
        .iter()
        .map(|c| (c.commit.summary.as_str(), c.signed))
        .collect();
    assert_eq!(signed, vec![("signed", true), ("init", false)]);
    // No verifier: `unverified` for a signed commit, `unsigned` for the rest.
    let checks = repo.verify(&[c.oid.clone(), "HEAD~1".into()]).unwrap();
    assert_eq!(checks[0].state, GitSignatureState::Unverified);
    assert_eq!(checks[0].kind, Some(GitSignatureKind::Ssh));
    assert_eq!(checks[1].state, GitSignatureState::Unsigned);
    // A plain repository (no `Signing` at all) answers the same.
    let plain = f.repo().verify(&[c.oid]).unwrap();
    assert_eq!(plain[0].state, GitSignatureState::Unverified);
}

#[test]
fn a_tampered_signature_is_bad() {
    let (f, signer) = signed_fixture("sign-bad");
    let repo = repo_with(&f, &signer);
    f.write("notes.txt", "a\n");
    let c = repo.commit("signed", &paths(&["notes.txt"]), None).unwrap();
    // Rewrite the message but keep the signature header (what a forger would do).
    let raw_commit = raw(f.dir(), &c.oid).replacen("\n\nsigned", "\n\nforged", 1);
    let forged = git_hash_object(f.dir(), &raw_commit);
    let checks = repo.verify(&[forged]).unwrap();
    assert_eq!(checks[0].state, GitSignatureState::Bad);
}

fn git_hash_object(dir: &std::path::Path, text: &str) -> String {
    use std::io::Write as _;
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "hash-object",
            "-t",
            "commit",
            "-w",
            "--stdin",
            "--literally",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_owned()
}

#[test]
fn a_signer_may_decline_and_may_fail() {
    let f = fixture("sign-decline");
    let skip = Arc::new(TestSigner {
        skip: true,
        ..TestSigner::default()
    });
    let repo = f.repo().with_signing(signing(skip, false));
    f.write("notes.txt", "a\n");
    let c = repo
        .commit("unsigned", &paths(&["notes.txt"]), None)
        .unwrap();
    assert!(!raw(f.dir(), &c.oid).contains("gpgsig"));

    // A failing signer aborts the commit: nothing is written, nothing moves.
    let refuse = Arc::new(TestSigner {
        refuse: true,
        ..TestSigner::default()
    });
    let repo = f.repo().with_signing(signing(refuse, false));
    f.write("notes.txt", "b\n");
    let head = ref_id(f.dir(), "refs/heads/main");
    let err = repo
        .commit("never", &paths(&["notes.txt"]), None)
        .map_err(AppError::from)
        .unwrap_err();
    assert!(err.to_string().contains("locked"), "{err}");
    assert_eq!(ref_id(f.dir(), "refs/heads/main"), head);
}

#[test]
fn without_signing_commits_are_unsigned_and_the_reflog_is_git_like() {
    let f = fixture("sign-none");
    f.write("notes.txt", "a\n");
    let c = f
        .repo()
        .commit("plain", &paths(&["notes.txt"]), None)
        .unwrap();
    assert!(!raw(f.dir(), &c.oid).contains("gpgsig"));
    assert_eq!(
        git(f.dir(), &["reflog", "-1", "--format=%gs"])
            .unwrap()
            .trim(),
        "commit: plain"
    );
    assert!(!f.repo().commit_info(&c.oid).unwrap().signed);
}

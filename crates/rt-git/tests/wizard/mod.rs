// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Helpers of the Git wizard tests (wave 14, strand G: `actions.rs`, `undo.rs`, `stash.rs`):
//! a repository with a collection and a plain text file, pushed to a bare remote; plan and
//! apply shortcuts; a fingerprint of everything a user can see of a repository.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use rt_app::git::actions::{GitAction, GitActionResult, GitBlockReason, GitPlan};
use rt_git::{ActionOptions, NoCredentials, Repo};

use crate::common::*;

pub const COLLECTION: &str = "opencollection: 1.0.0\ninfo:\n  name: Shared\n";

pub fn request(method: &str, url: &str) -> String {
    format!(
        "info:\n  name: Get user\n  type: http\n  seq: 1\n\nhttp:\n  method: {method}\n  url: {url}\n  auth: inherit\n\ndocs: |-\n  Reads one user.\n"
    )
}

pub const NOTES: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";

/// A work tree with a collection (`opencollection.yml`, `users/get.yml`) and `notes.txt`,
/// committed as "init" by Ada and pushed to a bare remote (`main` tracks `origin/main`).
pub struct Fixture {
    pub remote: Tmp,
    pub work: Tmp,
}

impl Fixture {
    pub fn dir(&self) -> &Path {
        self.work.path()
    }
    pub fn repo(&self) -> Repo {
        reopen(self.dir())
    }
    pub fn write(&self, rel: &str, text: &str) {
        write(self.dir(), rel, text)
    }
    pub fn read(&self, rel: &str) -> String {
        read(self.dir(), rel)
    }
    pub fn exists(&self, rel: &str) -> bool {
        self.dir().join(rel).exists()
    }
    /// Commits `files` (written first) on the current branch.
    pub fn commit(&self, message: &str, files: &[(&str, &str)]) -> String {
        for (p, t) in files {
            self.write(p, t);
        }
        let list: Vec<String> = files.iter().map(|(p, _)| (*p).to_owned()).collect();
        self.repo().commit(message, &list, None).unwrap().oid
    }
}

pub fn fixture(name: &str) -> Fixture {
    let remote = tmp(&format!("{name}-remote"));
    bare(remote.path());
    let work = tmp(&format!("{name}-work"));
    new_repo(work.path(), "Ada");
    add_remote(work.path(), remote.path().to_str().unwrap());
    let f = Fixture { remote, work };
    f.commit(
        "init",
        &[
            ("opencollection.yml", COLLECTION),
            ("users/get.yml", &request("GET", "https://api.test/users/1")),
            ("notes.txt", NOTES),
        ],
    );
    f.repo().push(&NoCredentials).unwrap();
    f
}

pub fn options(unsaved: bool) -> ActionOptions<'static> {
    ActionOptions {
        unsaved,
        credentials: &NoCredentials,
        identity: None,
    }
}

pub fn plan(repo: &Repo, action: &GitAction) -> GitPlan {
    repo.plan(action, false).unwrap()
}

pub fn apply(repo: &Repo, action: &GitAction) -> GitActionResult {
    repo.apply(action, &options(false))
        .unwrap_or_else(|e| panic!("{action:?}: {e}"))
}

pub fn reason(repo: &Repo, action: &GitAction) -> Option<GitBlockReason> {
    repo.plan(action, false).unwrap().blocked.map(|b| b.reason)
}

pub fn act(json: &str) -> GitAction {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"))
}

/// Every file of the work tree (not `.git`), by path.
pub fn files(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if e.file_name() == ".git" {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(
                    rel,
                    String::from_utf8_lossy(&std::fs::read(&p).unwrap()).into(),
                );
            }
        }
    }
    out
}

/// What a user sees of the repository: refs (without backups), `HEAD`, configuration,
/// stash list, merge state, status and the files.
pub fn state(dir: &Path) -> String {
    let repo = gix::open_opts(dir, gix::open::Options::isolated()).unwrap();
    let mut refs = BTreeMap::new();
    for r in repo.references().unwrap().all().unwrap() {
        let r = r.unwrap();
        let name = r.name().as_bstr().to_string();
        if name.starts_with("refs/requesttailor/") {
            continue;
        }
        refs.insert(name, format!("{:?}", r.target()));
    }
    let git = dir.join(".git");
    let text = |p: PathBuf| std::fs::read_to_string(p).unwrap_or_default();
    let status = reopen(dir).status().unwrap();
    format!(
        "refs: {refs:#?}\nHEAD: {}\nconfig: {}\nstash: {}\nmerge: {} {}\nstatus: {status:?}\nfiles: {:#?}\n",
        text(git.join("HEAD")),
        text(git.join("config")),
        reopen(dir)
            .stashes()
            .unwrap()
            .iter()
            .map(|s| format!("{} {} {:?}", s.oid, s.message, s.files))
            .collect::<Vec<_>>()
            .join("; "),
        git.join("MERGE_HEAD").exists(),
        git.join("CHERRY_PICK_HEAD").exists(),
        files(dir)
    )
}

/// [`state`] plus the bytes of the index and the journal: a plan must not change any of it.
pub fn raw_state(dir: &Path) -> String {
    let git = dir.join(".git");
    format!(
        "{}\nindex: {:?}\njournal: {:?}",
        state(dir),
        std::fs::read(git.join("index")).ok(),
        std::fs::read(git.join("requesttailor/actions.jsonl")).ok()
    )
}

/// A backup ref exists.
pub fn has_ref(dir: &Path, name: &str) -> bool {
    ref_id(dir, name).is_some()
}

/// `git` with an empty configuration, `None` when it is not installed.
pub fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", dir)
        .output()
        .ok()?;
    if !out.status.success() {
        panic!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

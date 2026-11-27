// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Fetch, clone and push over the SSH seam (N10). The connector of these tests is no SSH client:
//! it runs the requested service (`git upload-pack`, `git receive-pack`) of the installed `git`
//! on this machine and hands back its standard streams, which is what an SSH server does after
//! authentication. Skipped without `git`; the code under test never runs it. The product's
//! russh connector is tested against a real SSH server in GitTailor (`gt-ssh`).

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use common::*;
use tk_git::{GitError, NoCredentials, Repo, clone_with_ssh};
use tk_git_types::ssh::{SshConnector, SshStream, SshTarget};

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// Runs the service on this machine; `root` stands for the server's file system (the URL path
/// is below it). Records every command it was asked to run.
struct LocalGit {
    root: PathBuf,
    seen: Mutex<Vec<(SshTarget, String)>>,
}

impl LocalGit {
    fn new(root: &Path) -> Arc<Self> {
        Arc::new(Self {
            root: root.to_path_buf(),
            seen: Mutex::new(Vec::new()),
        })
    }
}

/// `git-upload-pack 'a/b.git'` → (`upload-pack`, `a/b.git`).
fn parse(command: &str) -> (String, String) {
    let (service, quoted) = command.split_once(' ').unwrap();
    let path = quoted
        .strip_prefix('\'')
        .and_then(|q| q.strip_suffix('\''))
        .unwrap()
        .replace(r"'\''", "'");
    (service.strip_prefix("git-").unwrap().to_owned(), path)
}

impl SshConnector for LocalGit {
    fn exec(&self, target: &SshTarget, command: &str) -> Result<SshStream, GitError> {
        self.seen
            .lock()
            .unwrap()
            .push((target.clone(), command.to_owned()));
        if target.host == "refusing.example" {
            return Err(GitError::Auth(target.host.clone()));
        }
        let (service, path) = parse(command);
        let mut child = Command::new("git")
            .arg(service)
            .arg(self.root.join(path.trim_start_matches('/')))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| GitError::Failed(e.to_string()))?;
        Ok(SshStream {
            reader: Box::new(child.stdout.take().unwrap()),
            writer: Box::new(child.stdin.take().unwrap()),
        })
    }
}

/// A bare `origin.git` below `root` with one commit (`a.txt`) on `main`, pushed from `author`.
fn origin(root: &Path, author: &Path) -> PathBuf {
    let origin = root.join("srv/origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    bare(&origin);
    drop(new_repo(author, "Ada"));
    write(author, "a.txt", "one\n");
    add_remote(author, origin.to_str().unwrap());
    let work = reopen(author);
    work.commit("first", &paths(&["a.txt"]), None).unwrap();
    work.push(&NoCredentials).unwrap();
    origin
}

fn with_ssh(dir: &Path, connector: &Arc<LocalGit>) -> Repo {
    reopen(dir).with_ssh(Some(connector.clone() as Arc<dyn SshConnector>))
}

#[test]
fn clone_fetch_and_push_over_the_connector() {
    if !have_git() {
        eprintln!("skipped: git is not installed");
        return;
    }
    let server = tmp("ssh-server");
    let author = tmp("ssh-author");
    let origin = origin(server.path(), author.path());
    let connector = LocalGit::new(server.path());

    // Clone, scp form.
    let parent = tmp("ssh-parent");
    let dir = clone_with_ssh(
        "git@example.com:srv/origin.git",
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
        Some(connector.clone()),
    )
    .unwrap();
    assert_eq!(read(&dir, "a.txt"), "one\n");
    assert_eq!(
        ref_id(&dir, "refs/heads/main"),
        ref_id(&origin, "refs/heads/main")
    );
    {
        let seen = connector.seen.lock().unwrap();
        let (target, command) = &seen[0];
        assert_eq!(target.user.as_deref(), Some("git"));
        assert_eq!(target.host, "example.com");
        assert_eq!(command, "git-upload-pack 'srv/origin.git'");
    }
    set_user(&dir, "Bob");

    // Fetch a new commit of the author.
    write(author.path(), "a.txt", "two\n");
    let work = reopen(author.path());
    work.commit("second", &paths(&["a.txt"]), None).unwrap();
    work.push(&NoCredentials).unwrap();
    let clone = with_ssh(&dir, &connector);
    clone.pull(&NoCredentials, None).unwrap();
    assert_eq!(read(&dir, "a.txt"), "two\n");
    assert_eq!(
        ref_id(&dir, "refs/remotes/origin/main"),
        ref_id(&origin, "refs/heads/main")
    );

    // Push a fast-forward, ssh:// form.
    std::fs::write(
        dir.join(".git/config"),
        std::fs::read_to_string(dir.join(".git/config"))
            .unwrap()
            .replace(
                "git@example.com:srv/origin.git",
                "ssh://git@example.com:2222/srv/origin.git",
            ),
    )
    .unwrap();
    write(&dir, "b.txt", "bob\n");
    let clone = with_ssh(&dir, &connector);
    clone.commit("from bob", &paths(&["b.txt"]), None).unwrap();
    clone.push(&NoCredentials).unwrap();
    assert_eq!(
        ref_id(&origin, "refs/heads/main"),
        ref_id(&dir, "refs/heads/main")
    );
    {
        let seen = connector.seen.lock().unwrap();
        let (target, command) = seen.last().unwrap();
        assert_eq!(target.port, Some(2222));
        assert_eq!(command, "git-receive-pack '/srv/origin.git'");
    }
    // Pushing again sends nothing and succeeds.
    clone.push(&NoCredentials).unwrap();

    // Not a fast-forward: refused, the remote unchanged.
    write(author.path(), "a.txt", "three\n");
    let work = reopen(author.path());
    work.pull(&NoCredentials, None).unwrap();
    work.commit("third", &paths(&["a.txt"]), None).unwrap();
    work.push(&NoCredentials).unwrap();
    let before = ref_id(&origin, "refs/heads/main");
    write(&dir, "b.txt", "bob again\n");
    clone.commit("diverged", &paths(&["b.txt"]), None).unwrap();
    let err = clone.push(&NoCredentials).unwrap_err();
    assert!(err.to_string().contains("pull first"), "{err}");
    assert_eq!(ref_id(&origin, "refs/heads/main"), before);
}

#[test]
fn a_new_branch_goes_to_an_empty_remote() {
    if !have_git() {
        eprintln!("skipped: git is not installed");
        return;
    }
    let server = tmp("ssh-empty");
    let origin = server.path().join("empty.git");
    std::fs::create_dir_all(&origin).unwrap();
    bare(&origin);
    let connector = LocalGit::new(server.path());
    let parent = tmp("ssh-empty-parent");
    let dir = clone_with_ssh(
        "ssh://example.com/empty.git",
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
        Some(connector.clone()),
    )
    .unwrap();
    set_user(&dir, "Eve");
    write(&dir, "x.txt", "x\n");
    let repo = with_ssh(&dir, &connector);
    repo.commit("first", &paths(&["x.txt"]), None).unwrap();
    repo.push(&NoCredentials).unwrap();
    assert_eq!(
        ref_id(&origin, "refs/heads/main"),
        ref_id(&dir, "refs/heads/main")
    );
}

#[test]
fn errors_of_the_connector_reach_the_caller_and_no_connector_is_unsupported() {
    let work = tmp("ssh-errors");
    new_repo(work.path(), "Ada");
    add_remote(work.path(), "git@refusing.example:a/b.git");
    write(work.path(), "a.txt", "a\n");
    let repo = reopen(work.path());
    repo.commit("first", &paths(&["a.txt"]), None).unwrap();
    assert!(matches!(
        repo.push(&NoCredentials),
        Err(GitError::Unsupported(_))
    ));
    let connector = LocalGit::new(work.path());
    let repo = with_ssh(work.path(), &connector);
    assert!(matches!(
        repo.push(&NoCredentials),
        Err(GitError::Auth(host)) if host == "refusing.example"
    ));
    assert!(matches!(
        repo.pull(&NoCredentials, None),
        Err(GitError::Auth(host)) if host == "refusing.example"
    ));
    // Cloning leaves nothing behind.
    let parent = tmp("ssh-errors-parent");
    let err = clone_with_ssh(
        "git@refusing.example:a/b.git",
        parent.path(),
        None,
        &NoCredentials,
        &opts(),
        Some(connector),
    )
    .unwrap_err();
    assert!(matches!(err, GitError::Auth(_)), "{err:?}");
    assert!(!parent.path().join("b").exists());
}

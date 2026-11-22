// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Shared helpers of the rt-git integration tests: temporary folders, repositories with a
//! test identity, bare remotes, an in-memory secret store, and a smart HTTP server around
//! `git http-backend` (only when `git` is installed; the code under test never runs it).

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use rt_app::{AppError, SecretKey, SecretStore};
use rt_git::{Options, Repo};

pub fn opts() -> Options {
    Options { isolated: true }
}

pub struct Tmp(pub PathBuf);

impl Tmp {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn tmp(name: &str) -> Tmp {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rt-git-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Tmp(std::fs::canonicalize(&dir).unwrap())
}

pub fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

pub fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

pub fn append_config(work_or_git_dir: &Path, text: &str) {
    let git = work_or_git_dir.join(".git");
    let config = if git.is_dir() { git.join("config") } else { work_or_git_dir.join("config") };
    let mut f = std::fs::OpenOptions::new().append(true).open(config).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

/// Sets the commit identity in the repository's own configuration.
pub fn set_user(dir: &Path, name: &str) {
    append_config(dir, &format!("[user]\n\tname = {name}\n\temail = {}@example.com\n", name.to_lowercase()));
}

/// A new repository in `dir` with a test identity.
pub fn new_repo(dir: &Path, user: &str) -> Repo {
    rt_git::init(dir, &opts()).unwrap();
    set_user(dir, user);
    Repo::require(dir, &opts()).unwrap()
}

pub fn reopen(dir: &Path) -> Repo {
    Repo::require(dir, &opts()).unwrap()
}

/// A bare repository at `dir` (accepts pushes over `git http-backend` too).
pub fn bare(dir: &Path) {
    gix::init_bare(dir).unwrap();
    append_config(dir, "[http]\n\treceivepack = true\n");
}

pub fn add_remote(dir: &Path, url: &str) {
    append_config(
        dir,
        &format!("[remote \"origin\"]\n\turl = {url}\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n"),
    );
}

/// The commit a ref points to in the repository at `dir` (bare or not).
pub fn ref_id(dir: &Path, name: &str) -> Option<String> {
    let repo = gix::open_opts(dir, gix::open::Options::isolated()).unwrap();
    let mut r = repo.try_find_reference(name).unwrap()?;
    Some(r.peel_to_id().unwrap().to_string())
}

pub fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// In-memory stand-in for the keychain.
#[derive(Default)]
pub struct MemorySecrets(pub Mutex<HashMap<SecretKey, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, AppError> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &SecretKey, value: &str) -> Result<(), AppError> {
        self.0.lock().unwrap().insert(key.clone(), value.to_owned());
        Ok(())
    }
    fn delete(&self, key: &SecretKey) -> Result<(), AppError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

// --- smart HTTP test server -------------------------------------------------------------

/// User and password the `/auth/` area of the server accepts.
pub const USER: &str = "alice";
pub const PASSWORD: &str = "secret";
/// `Basic` value of `alice:secret`.
const BASIC: &str = "Basic YWxpY2U6c2VjcmV0";

pub struct GitHttp {
    pub port: u16,
    /// `GIT_PROJECT_ROOT`: bare repositories live here.
    pub root: PathBuf,
}

impl GitHttp {
    pub fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
    /// URL of the repository `name` (a folder below `root`); `auth`: behind Basic auth.
    pub fn url(&self, name: &str, auth: bool) -> String {
        let area = if auth { "auth" } else { "open" };
        format!("http://127.0.0.1:{}/{area}/{name}", self.port)
    }
}

/// The shared server of this test binary, `None` (and a message) if `git` is not installed.
/// Ports 47821–47829 belong to this strand.
pub fn git_http() -> Option<&'static GitHttp> {
    static SERVER: OnceLock<Option<GitHttp>> = OnceLock::new();
    SERVER
        .get_or_init(|| {
            let exec = Command::new("git").arg("--exec-path").output().ok()?;
            if !exec.status.success() {
                return None;
            }
            let backend = PathBuf::from(String::from_utf8(exec.stdout).ok()?.trim()).join("git-http-backend");
            if !backend.exists() {
                return None;
            }
            let root = std::env::temp_dir().join(format!("rt-git-http-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).ok()?;
            let root = std::fs::canonicalize(root).ok()?;
            let (listener, port) = (47821..=47829).find_map(|port| {
                TcpListener::bind(("127.0.0.1", port)).ok().map(|l| (l, port))
            })?;
            let served = root.clone();
            std::thread::spawn(move || {
                for conn in listener.incoming().flatten() {
                    let backend = backend.clone();
                    let root = served.clone();
                    std::thread::spawn(move || {
                        let _ = serve(conn, &backend, &root);
                    });
                }
            });
            Some(GitHttp { port, root })
        })
        .as_ref()
}

/// Skips the calling test (with a message) when there is no `git` for the server.
#[macro_export]
macro_rules! require_git_http {
    () => {
        match common::git_http() {
            Some(server) => server,
            None => {
                eprintln!("SKIPPED: `git` (git http-backend) is not installed; smart HTTP tests need it for the test server");
                return;
            }
        }
    };
}

fn serve(conn: TcpStream, backend: &Path, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let mut headers = HashMap::new();
    loop {
        let mut h = String::new();
        reader.read_line(&mut h)?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
        }
    }
    let mut body = Vec::new();
    if let Some(len) = headers.get("content-length") {
        body.resize(len.parse().unwrap_or(0), 0);
        reader.read_exact(&mut body)?;
    } else if headers.get("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        loop {
            let mut size = String::new();
            reader.read_line(&mut size)?;
            let size = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
            let mut chunk = vec![0; size + 2];
            reader.read_exact(&mut chunk)?;
            if size == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..size]);
        }
    }
    let mut out = conn;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let (auth, path_info) = if let Some(rest) = path.strip_prefix("/auth") {
        (true, rest)
    } else if let Some(rest) = path.strip_prefix("/open") {
        (false, rest)
    } else {
        out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
        return Ok(());
    };
    if auth && headers.get("authorization").map(String::as_str) != Some(BASIC) {
        out.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"test\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
        return Ok(());
    }
    let mut cmd = Command::new(backend);
    cmd.env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_PROJECT_ROOT", root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", root)
        .env("REQUEST_METHOD", &method)
        .env("PATH_INFO", path_info)
        .env("QUERY_STRING", query)
        .env("CONTENT_LENGTH", body.len().to_string())
        .env("REMOTE_ADDR", "127.0.0.1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(ct) = headers.get("content-type") {
        cmd.env("CONTENT_TYPE", ct);
    }
    if let Some(enc) = headers.get("content-encoding") {
        cmd.env("HTTP_CONTENT_ENCODING", enc);
    }
    if let Some(proto) = headers.get("git-protocol") {
        cmd.env("GIT_PROTOCOL", proto);
    }
    if auth {
        cmd.env("REMOTE_USER", USER);
    }
    let mut child = cmd.spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&body);
    });
    let mut cgi = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut cgi)?;
    let _ = writer.join();
    let _ = child.wait();
    let split = cgi
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| (p, 4))
        .or_else(|| cgi.windows(2).position(|w| w == b"\n\n").map(|p| (p, 2)));
    let Some((end, sep)) = split else {
        out.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
        return Ok(());
    };
    let head = String::from_utf8_lossy(&cgi[..end]).into_owned();
    let payload = &cgi[end + sep..];
    let mut status = "200 OK".to_owned();
    let mut response = Vec::new();
    for h in head.lines() {
        if let Some(s) = h.strip_prefix("Status:") {
            status = s.trim().to_owned();
        } else if !h.trim().is_empty() {
            response.extend_from_slice(h.trim().as_bytes());
            response.extend_from_slice(b"\r\n");
        }
    }
    let mut msg = format!("HTTP/1.1 {status}\r\n").into_bytes();
    msg.extend(response);
    msg.extend(format!("Content-Length: {}\r\nConnection: close\r\n\r\n", payload.len()).into_bytes());
    msg.extend_from_slice(payload);
    out.write_all(&msg)?;
    Ok(())
}

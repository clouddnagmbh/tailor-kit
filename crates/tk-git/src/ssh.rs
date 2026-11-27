// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Fetch and push over SSH (seam N10). The product's [`SshConnector`] opens the connection and
//! runs `git-upload-pack` or `git-receive-pack` on the server; this module speaks Git's protocol
//! over the two byte streams it hands back and never sees a key, a passphrase or a host key.
//!
//! - Fetch: gitoxide's own client over the streams (its `git://` connection in process mode,
//!   the way `ssh` runs a spawned `git-upload-pack`). Protocol v0/v1: asking for v2 needs the
//!   `GIT_PROTOCOL` environment variable on the server, which a connector cannot be expected to
//!   set; every server answers v0.
//! - Push: our own client, as over HTTP ([`crate::http`]): the ref advertisement, a
//!   fast-forward check, one update command with `report-status`, the pack, the report. Only
//!   the transport differs: one bidirectional stream instead of two requests.

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use tk_git_types::GitError;
use tk_git_types::ssh::{SshConnector, SshStream, SshTarget};

use crate::http::{PushRequest, check_report, parse_advertisement, pkt_line, prepare_push};
use crate::util::{Context, Result, fail};

/// `host`, or `host:port` when the URL names a port: what messages and `GitError::Auth` carry
/// (as for HTTP remotes).
pub fn host_label(target: &SshTarget) -> String {
    match target.port {
        Some(port) if target.host.contains(':') => format!("[{}]:{port}", target.host),
        Some(port) => format!("{}:{port}", target.host),
        None => target.host.clone(),
    }
}

/// The remote command line, the path quoted for a POSIX shell the way `git` quotes it.
pub fn command(service: &str, path: &str) -> String {
    format!("{service} '{}'", path.replace('\'', r"'\''"))
}

fn exec(connector: &dyn SshConnector, target: &SshTarget, service: &str) -> Result<SshStream> {
    if target.path.is_empty() {
        return Err(fail(format!(
            "the SSH remote on {} names no repository path",
            host_label(target)
        )));
    }
    connector.exec(target, &command(service, &target.path))
}

/// Fetches through `remote` (configured in the repository, with the refspecs it carries) from
/// `target`. Returns the branch the remote's `HEAD` points to, when it says so.
pub(crate) fn fetch(
    remote: gix::Remote<'_>,
    target: &SshTarget,
    connector: &dyn SshConnector,
    options: gix::remote::ref_map::Options,
) -> Result<Option<String>> {
    let stream = exec(connector, target, "git-upload-pack")?;
    let host = host_label(target);
    let broken = Arc::new(Mutex::new(None));
    let transport = gix::protocol::transport::client::git::blocking_io::Connection::new(
        Watch {
            inner: stream.reader,
            broken: broken.clone(),
        },
        stream.writer,
        gix::protocol::transport::Protocol::V1,
        target.path.as_str(),
        None::<(&str, Option<u16>)>,
        gix::protocol::transport::client::git::ConnectMode::Process,
        false,
    )
    .custom_url(Some(
        format!("ssh://{host}/{}", target.path.trim_start_matches('/')).into(),
    ));
    let ref_map = (|| -> gix::Result<Option<gix::remote::fetch::RefMap>> {
        let prepare = remote
            .to_connection_with_transport(transport)
            .prepare_fetch(gix::progress::Discard, options)?;
        // An empty repository advertises nothing, not even capabilities (protocol v0), and
        // gitoxide would refuse to negotiate without `include-tag`: there is nothing to fetch.
        if prepare.ref_map().remote_refs.is_empty() {
            return Ok(None);
        }
        let outcome = prepare.receive(gix::progress::Discard, &AtomicBool::new(false))?;
        Ok(Some(outcome.ref_map))
    })()
    .map_err(|e| {
        // What the connection said (the server's error output) says more than gitoxide's
        // summary of it.
        match broken.lock().unwrap_or_else(|p| p.into_inner()).take() {
            Some(why) => fail(format!("cannot fetch from {host}: {why}")),
            None => fail(format!("cannot fetch from {host}: {e}")),
        }
    })?;
    let Some(ref_map) = ref_map else {
        return Ok(None);
    };
    Ok(ref_map.remote_refs.iter().find_map(|r| match r {
        gix::protocol::handshake::Ref::Symbolic {
            full_ref_name,
            target,
            ..
        } if full_ref_name == "HEAD" => target
            .strip_prefix(b"refs/heads/")
            .and_then(|b| b.to_str().ok())
            .map(str::to_owned),
        _ => None,
    }))
}

/// Keeps the first error the remote command's output stream reports.
struct Watch {
    inner: Box<dyn Read + Send>,
    broken: Arc<Mutex<Option<String>>>,
}

impl Read for Watch {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf).inspect_err(|e| {
            self.broken
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get_or_insert_with(|| e.to_string());
        })
    }
}

/// Fetches `remote_name` (configured in the repository) from `target`.
pub(crate) fn fetch_remote(
    repo: &gix::Repository,
    remote_name: &str,
    target: &SshTarget,
    connector: &dyn SshConnector,
    with_head: bool,
) -> Result<Option<String>> {
    let remote = repo
        .find_remote(remote_name)
        .ctx(&format!("no remote named {remote_name}"))?;
    let mut options = gix::remote::ref_map::Options::default();
    if with_head {
        options.extra_refspecs.push(
            gix::refspec::parse("HEAD".into(), gix::refspec::parse::Operation::Fetch)
                .expect("valid refspec")
                .to_owned(),
        );
    }
    fetch(remote, target, connector, options)
}

/// Reads one pkt-line: `None` for a flush (and the other special packets), else its payload.
fn read_pkt(reader: &mut dyn Read, host: &str) -> Result<Option<Vec<u8>>> {
    let lost = |e: std::io::Error| fail(format!("the connection to {host} broke off: {e}"));
    let mut len = [0u8; 4];
    reader.read_exact(&mut len).map_err(lost)?;
    let len = std::str::from_utf8(&len)
        .ok()
        .and_then(|l| usize::from_str_radix(l, 16).ok())
        .ok_or_else(|| fail(format!("{host} sent an invalid response")))?;
    if len < 4 {
        return Ok(None);
    }
    let mut payload = vec![0u8; len - 4];
    reader.read_exact(&mut payload).map_err(lost)?;
    Ok(Some(payload))
}

/// Pushes `new` to `remote_ref` (e.g. `refs/heads/main`) of the SSH remote `target`, as `agent`
/// (the `agent=` capability). Fast-forward only, as over HTTP.
pub(crate) fn push(
    repo: &gix::Repository,
    target: &SshTarget,
    connector: &dyn SshConnector,
    new: ObjectId,
    remote_ref: &str,
    agent: &str,
) -> Result<()> {
    let SshStream {
        mut reader,
        mut writer,
    } = exec(connector, target, "git-receive-pack")?;
    let host = host_label(target);
    let lost = |e: std::io::Error| fail(format!("the connection to {host} broke off: {e}"));

    // The advertisement ends with the first flush; keep it as pkt-lines for the HTTP parser.
    let mut body = Vec::new();
    while let Some(line) = read_pkt(&mut reader, &host)? {
        body.extend(pkt_line(&line));
    }
    body.extend_from_slice(b"0000");
    let adv = parse_advertisement(&body)?;

    let request = match prepare_push(repo, &adv, new, remote_ref, agent) {
        Ok(PushRequest::Send(payload)) => payload,
        done => {
            // A flush tells the server that no update follows; it then ends cleanly.
            let _ = writer.write_all(b"0000").and_then(|()| writer.flush());
            return done.map(|_| ());
        }
    };
    writer
        .write_all(&request)
        .and_then(|()| writer.flush())
        .map_err(lost)?;
    // End of input: the server stores the pack, then reports.
    drop(writer);
    let mut report = Vec::new();
    reader.read_to_end(&mut report).map_err(lost)?;
    if adv.capabilities.iter().any(|c| c == "report-status") {
        check_report(&report, remote_ref).map_err(|e| match e {
            GitError::Failed(m) => fail(format!("{host}: {m}")),
            other => other,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_quote_the_path_for_the_shell() {
        assert_eq!(
            command("git-upload-pack", "a/b.git"),
            "git-upload-pack 'a/b.git'"
        );
        assert_eq!(
            command("git-receive-pack", "/srv/it's.git"),
            r"git-receive-pack '/srv/it'\''s.git'"
        );
    }

    #[test]
    fn host_labels_carry_the_port() {
        let t = |host: &str, port| SshTarget {
            user: None,
            host: host.into(),
            port,
            path: "x".into(),
        };
        assert_eq!(host_label(&t("github.com", None)), "github.com");
        assert_eq!(host_label(&t("127.0.0.1", Some(2222))), "127.0.0.1:2222");
        assert_eq!(host_label(&t("::1", Some(22))), "[::1]:22");
    }
}

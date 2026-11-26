// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Push over Git's smart HTTP protocol, our own client because gitoxide cannot push yet.
//!
//! One branch per push, the way `git push` speaks to `git-receive-pack` (protocol v0):
//!
//! 1. `GET <url>/info/refs?service=git-receive-pack`: the ref advertisement (pkt-lines,
//!    capabilities after a NUL on the first line).
//! 2. Locally: refuse unless the remote branch is an ancestor of ours (fast-forward only);
//!    collect the objects the remote lacks (everything not reachable from advertised commits
//!    we know) and write them as a pack ([`crate::pack`]).
//! 3. `POST <url>/git-receive-pack`: one update command `<old> <new> <ref>` with
//!    `report-status`, a flush, the pack.
//! 4. Parse `report-status`: `unpack ok`, then `ok <ref>` or `ng <ref> <reason>`.
//!
//! A `401` without usable credentials is `GitError::Auth(host)`; with credentials the
//! call is repeated once with HTTP Basic authentication. Credentials go over plain
//! `http://` only to loopback hosts.

use std::collections::{BTreeMap, HashSet};

use gix::ObjectId;

use crate::credentials::CredentialSource;
use crate::local::{is_ancestor, missing_objects, tree_objects};
use crate::net::{Auth, HttpTarget, ensure_tls};
use crate::util::{Context, Result, fail};

/// Splits `data` into pkt-lines; `None` marks a flush (`0000`).
pub fn pkt_lines(mut data: &[u8]) -> Result<Vec<Option<&[u8]>>> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let bad = || fail("the server sent an invalid response");
        let len = data.get(..4).ok_or_else(bad)?;
        let len = std::str::from_utf8(len).map_err(|_| bad())?;
        let len = usize::from_str_radix(len, 16).map_err(|_| bad())?;
        if len < 4 {
            // flush (0), delimiter (1), response end (2): all end a section here.
            out.push(None);
            data = &data[4..];
            continue;
        }
        let line = data.get(4..len).ok_or_else(bad)?;
        out.push(Some(line));
        data = &data[len..];
    }
    Ok(out)
}

/// One pkt-line holding `payload`.
pub fn pkt_line(payload: &[u8]) -> Vec<u8> {
    let mut out = format!("{:04x}", payload.len() + 4).into_bytes();
    out.extend_from_slice(payload);
    out
}

/// The receive-pack ref advertisement.
#[derive(Debug, Default)]
pub struct Advertisement {
    pub refs: BTreeMap<String, ObjectId>,
    pub capabilities: Vec<String>,
}

pub fn parse_advertisement(body: &[u8]) -> Result<Advertisement> {
    let mut lines = pkt_lines(body)?.into_iter().peekable();
    // Smart HTTP starts with `# service=git-receive-pack` and a flush.
    if let Some(Some(first)) = lines.peek()
        && first.starts_with(b"# service=")
    {
        lines.next();
        if let Some(None) = lines.peek() {
            lines.next();
        }
    }
    let mut adv = Advertisement::default();
    let mut first = true;
    for line in lines {
        let Some(line) = line else { break };
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let (refpart, caps) = match line.iter().position(|b| *b == 0) {
            Some(nul) => (&line[..nul], Some(&line[nul + 1..])),
            None => (line, None),
        };
        if first {
            if let Some(caps) = caps {
                adv.capabilities = String::from_utf8_lossy(caps)
                    .split(' ')
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            first = false;
        }
        let text =
            std::str::from_utf8(refpart).map_err(|_| fail("the server sent an invalid ref"))?;
        let (oid, name) = text
            .split_once(' ')
            .ok_or_else(|| fail("the server sent an invalid ref"))?;
        if name == "capabilities^{}" {
            continue;
        }
        let oid = ObjectId::from_hex(oid.as_bytes())
            .map_err(|_| fail("the server sent an invalid ref"))?;
        adv.refs.insert(name.to_owned(), oid);
    }
    Ok(adv)
}

/// Checks a `report-status` response for `remote_ref`.
pub fn check_report(body: &[u8], remote_ref: &str) -> Result<()> {
    let mut unpack_ok = false;
    for line in pkt_lines(body)?.into_iter().flatten() {
        let line = String::from_utf8_lossy(line);
        let line = line.trim_end_matches('\n');
        if let Some(status) = line.strip_prefix("unpack ") {
            if status != "ok" {
                return Err(fail(format!(
                    "the server could not store the pushed objects: {status}"
                )));
            }
            unpack_ok = true;
        } else if let Some(rest) = line.strip_prefix("ng ") {
            let (name, reason) = rest.split_once(' ').unwrap_or((rest, "rejected"));
            if name == remote_ref {
                return Err(fail(format!("the server rejected the push: {reason}")));
            }
        } else if let Some(name) = line.strip_prefix("ok ")
            && name == remote_ref
        {
            if !unpack_ok {
                return Err(fail("the server sent an incomplete push report"));
            }
            return Ok(());
        }
    }
    Err(fail("the server did not confirm the push"))
}

struct Client<'a> {
    http: reqwest::blocking::Client,
    target: &'a HttpTarget,
    base: String,
    auth: Auth,
    /// Credentials were needed once: send them with every following call.
    use_auth: bool,
    /// The product's user agent ([`crate::Product::agent`]).
    agent: &'a str,
}

impl Client<'_> {
    fn send(
        &mut self,
        build: impl Fn(&reqwest::blocking::Client) -> reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response> {
        loop {
            let mut req = build(&self.http)
                .header("User-Agent", self.agent)
                .header("Git-Protocol", "version=0");
            if self.use_auth
                && let Auth::Some(c) = &self.auth
            {
                req = req.basic_auth(&c.username, Some(&c.password));
            }
            let resp = req
                .send()
                .map_err(|e| fail(format!("cannot reach {}: {e}", self.target.host)))?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                if !self.use_auth && matches!(self.auth, Auth::Some(_)) {
                    self.use_auth = true;
                    continue;
                }
                return Err(self.target.auth_error(&self.auth));
            }
            if !status.is_success() {
                return Err(fail(format!("{} answered {status}", self.target.host)));
            }
            return Ok(resp);
        }
    }
}

/// Pushes `new` to `remote_ref` (e.g. `refs/heads/main`) of the smart HTTP remote `target`,
/// as `agent` (the user agent and the `agent=` capability).
pub fn push(
    repo: &gix::Repository,
    target: &HttpTarget,
    creds: &dyn CredentialSource,
    new: ObjectId,
    remote_ref: &str,
    agent: &str,
) -> Result<()> {
    ensure_tls();
    let http = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| fail(format!("cannot create the HTTP client: {e}")))?;
    let mut client = Client {
        http,
        target,
        base: target.url.trim_end_matches('/').to_owned(),
        auth: target.auth(creds)?,
        use_auth: false,
        agent,
    };

    let url = format!("{}/info/refs?service=git-receive-pack", client.base);
    let resp = client.send(|c| c.get(&url))?;
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = resp
        .bytes()
        .map_err(|e| fail(format!("cannot read the server's answer: {e}")))?;
    if content_type != "application/x-git-receive-pack-advertisement" {
        return Err(fail(format!(
            "{} does not accept pushes over smart HTTP",
            target.host
        )));
    }
    let adv = parse_advertisement(&body)?;

    let current = adv.refs.get(remote_ref).copied();
    if current == Some(new) {
        return Ok(());
    }
    if let Some(cur) = current
        && (!repo.has_object(cur) || !is_ancestor(repo, cur, new))
    {
        return Err(fail(
            "the remote branch has commits you do not have: pull first",
        ));
    }

    // Everything reachable from advertised commits we know is on the remote.
    let known: Vec<ObjectId> = adv
        .refs
        .values()
        .copied()
        .filter(|id| repo.find_commit(*id).is_ok())
        .collect();
    let mut remote_has: HashSet<ObjectId> = HashSet::new();
    if !known.is_empty() {
        let walk = repo
            .rev_walk(known.iter().copied())
            .all()
            .ctx("cannot walk the history")?;
        for info in walk {
            remote_has.insert(info.ctx("cannot walk the history")?.id);
        }
        // The trees of the advertised tips, so unchanged files are not sent again.
        let mut seen = HashSet::new();
        let mut tip_objects = Vec::new();
        for id in &known {
            let tree = repo
                .find_commit(*id)
                .ctx("cannot read a commit")?
                .tree_id()
                .ctx("cannot read a commit")?
                .detach();
            tree_objects(repo, tree, &|_| false, &mut seen, &mut tip_objects)?;
        }
        remote_has.extend(tip_objects);
    }
    let ids = missing_objects(repo, &[new], &|id| remote_has.contains(id))?;
    let pack = crate::pack::write(repo, &ids)?;

    let zero = ObjectId::null(repo.object_hash());
    let mut caps = Vec::new();
    for wanted in ["report-status"] {
        if adv.capabilities.iter().any(|c| c == wanted) {
            caps.push(wanted.to_owned());
        }
    }
    caps.push(format!("agent={agent}"));
    let command = format!(
        "{} {} {}\0{}\n",
        current.unwrap_or(zero),
        new,
        remote_ref,
        caps.join(" ")
    );
    let mut payload = pkt_line(command.as_bytes());
    payload.extend_from_slice(b"0000");
    payload.extend_from_slice(&pack);

    let url = format!("{}/git-receive-pack", client.base);
    let resp = client.send(|c| {
        c.post(&url)
            .header("Content-Type", "application/x-git-receive-pack-request")
            .header("Accept", "application/x-git-receive-pack-result")
            .body(payload.clone())
    })?;
    let body = resp
        .bytes()
        .map_err(|e| fail(format!("cannot read the server's answer: {e}")))?;
    if adv.capabilities.iter().any(|c| c == "report-status") {
        check_report(&body, remote_ref)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_advertisement_with_capabilities() {
        let mut body = pkt_line(b"# service=git-receive-pack\n");
        body.extend_from_slice(b"0000");
        let a = "1111111111111111111111111111111111111111";
        let b = "2222222222222222222222222222222222222222";
        body.extend(pkt_line(
            format!("{a} refs/heads/main\0report-status delete-refs side-band-64k\n").as_bytes(),
        ));
        body.extend(pkt_line(format!("{b} refs/heads/dev\n").as_bytes()));
        body.extend_from_slice(b"0000");
        let adv = parse_advertisement(&body).unwrap();
        assert_eq!(adv.refs.len(), 2);
        assert_eq!(adv.refs["refs/heads/main"].to_string(), a);
        assert!(adv.capabilities.iter().any(|c| c == "report-status"));
    }

    #[test]
    fn parses_the_advertisement_of_an_empty_repository() {
        let zero = "0000000000000000000000000000000000000000";
        let mut body = pkt_line(format!("{zero} capabilities^{{}}\0report-status\n").as_bytes());
        body.extend_from_slice(b"0000");
        let adv = parse_advertisement(&body).unwrap();
        assert!(adv.refs.is_empty());
        assert_eq!(adv.capabilities, ["report-status"]);
    }

    #[test]
    fn report_status() {
        let mut ok = pkt_line(b"unpack ok\n");
        ok.extend(pkt_line(b"ok refs/heads/main\n"));
        ok.extend_from_slice(b"0000");
        check_report(&ok, "refs/heads/main").unwrap();

        let mut ng = pkt_line(b"unpack ok\n");
        ng.extend(pkt_line(b"ng refs/heads/main non-fast-forward\n"));
        ng.extend_from_slice(b"0000");
        let err = check_report(&ng, "refs/heads/main").unwrap_err();
        assert!(err.to_string().contains("non-fast-forward"), "{err}");

        let mut bad = pkt_line(b"unpack index-pack failed\n");
        bad.extend_from_slice(b"0000");
        assert!(check_report(&bad, "refs/heads/main").is_err());
        assert!(pkt_lines(b"00zz").is_err());
    }
}

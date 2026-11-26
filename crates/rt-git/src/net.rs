// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Where a remote lives, which credentials may go there, and fetching over HTTP(S) with
//! gitoxide.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gix::bstr::ByteSlice;

use crate::credentials::{CredentialSource, Credentials};
use crate::util::{Context, GitError, Result, fail};

/// rustls on ring (not aws-lc) for every HTTPS client in this crate (reqwest is built
/// without a provider of its own).
pub fn ensure_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// An HTTP(S) remote.
#[derive(Debug, Clone)]
pub struct HttpTarget {
    /// The URL as configured (may carry a user name, never a password from us).
    pub url: String,
    /// `host` or `host:port`: the credential key and the `GitAuth` value.
    pub host: String,
    pub https: bool,
    pub loopback: bool,
}

#[derive(Debug, Clone)]
pub enum Endpoint {
    /// A repository on this machine (local path or `file://`).
    Local(PathBuf),
    Http(HttpTarget),
}

/// Parses a remote URL; relative local paths are relative to `base`.
pub fn endpoint(url: &str, base: &Path) -> Result<Endpoint> {
    let parsed = gix::url::parse(url.as_bytes().as_bstr())
        .map_err(|e| fail(format!("invalid remote URL {url:?}: {e}")))?;
    match parsed.scheme {
        gix::url::Scheme::File => {
            let path = gix::path::from_bstr(parsed.path.as_bstr())
                .map_err(|_| fail(format!("invalid remote path {url:?}")))?
                .into_owned();
            Ok(Endpoint::Local(if path.is_absolute() {
                path
            } else {
                base.join(path)
            }))
        }
        gix::url::Scheme::Http | gix::url::Scheme::Https => {
            let host = parsed
                .host()
                .ok_or_else(|| fail(format!("the remote URL {url:?} has no host")))?
                .to_owned();
            let loopback = host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback());
            let host = match parsed.port {
                Some(port) if host.contains(':') && !host.starts_with('[') => {
                    format!("[{host}]:{port}")
                }
                Some(port) => format!("{host}:{port}"),
                None => host,
            };
            Ok(Endpoint::Http(HttpTarget {
                url: url.to_owned(),
                host,
                https: parsed.scheme == gix::url::Scheme::Https,
                loopback,
            }))
        }
        other => Err(GitError::Unsupported(format!(
            "{other} remotes are not supported yet; use an https:// URL"
        ))),
    }
}

/// What may be sent to a host.
pub enum Auth {
    None,
    Some(Credentials),
    /// Credentials exist, but the connection is plain HTTP to another machine.
    Refused,
}

impl HttpTarget {
    pub fn auth(&self, creds: &dyn CredentialSource) -> Result<Auth> {
        Ok(match creds.get(&self.host)? {
            None => Auth::None,
            Some(_) if !self.https && !self.loopback => Auth::Refused,
            Some(c) => Auth::Some(c),
        })
    }

    /// The error for "the server wants credentials and we have none it may get".
    pub fn auth_error(&self, auth: &Auth) -> GitError {
        match auth {
            Auth::Refused => fail(format!(
                "refusing to send credentials over unencrypted http:// to {}; use an https:// URL",
                self.host
            )),
            _ => GitError::Auth(self.host.clone()),
        }
    }
}

/// Fetches `remote_name` (configured in the repository) over HTTP(S). Returns the branch the
/// remote's `HEAD` points to, when it says so.
pub fn fetch(
    repo: &gix::Repository,
    remote_name: &str,
    target: &HttpTarget,
    creds: &dyn CredentialSource,
    with_head: bool,
) -> Result<Option<String>> {
    ensure_tls();
    let auth = target.auth(creds)?;
    let remote = repo
        .find_remote(remote_name)
        .ctx(&format!("no remote named {remote_name}"))?;
    let asked = Cell::new(false);
    let result = (|| -> gix::Result<gix::remote::fetch::Outcome> {
        let connection = remote
            .connect(gix::remote::Direction::Fetch)?
            .with_credentials(|action| match action {
                gix::credentials::helper::Action::Get(ctx) => {
                    asked.set(true);
                    match &auth {
                        Auth::Some(c) => Ok(Some(gix::credentials::protocol::Outcome {
                            identity: gix::sec::identity::Account {
                                username: c.username.clone(),
                                password: c.password.clone(),
                                oauth_refresh_token: None,
                            },
                            next: ctx.into(),
                        })),
                        _ => Err(gix::Error::from_error(gix::error::message(
                            "authentication required",
                        ))),
                    }
                }
                _ => Ok(None),
            });
        let mut options = gix::remote::ref_map::Options::default();
        if with_head {
            options.extra_refspecs.push(
                gix::refspec::parse("HEAD".into(), gix::refspec::parse::Operation::Fetch)
                    .expect("valid refspec")
                    .to_owned(),
            );
        }
        connection
            .prepare_fetch(gix::progress::Discard, options)?
            .receive(gix::progress::Discard, &AtomicBool::new(false))
    })();
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(_) if asked.get() => return Err(target.auth_error(&auth)),
        Err(e) => {
            return Err(fail(format!("cannot fetch from {}: {e}", target.host)));
        }
    };
    Ok(outcome.ref_map.remote_refs.iter().find_map(|r| match r {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::NoCredentials;
    use rt_app::AppError;

    struct One;
    impl CredentialSource for One {
        fn get(&self, _host: &str) -> Result<Option<Credentials>, AppError> {
            Ok(Some(Credentials {
                username: "u".into(),
                password: "p".into(),
            }))
        }
    }

    fn http(url: &str) -> HttpTarget {
        match endpoint(url, Path::new("/base")).unwrap() {
            Endpoint::Http(t) => t,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn hosts_carry_the_port_when_the_url_names_one() {
        assert_eq!(http("https://github.com/a/b.git").host, "github.com");
        assert_eq!(http("http://127.0.0.1:8080/x").host, "127.0.0.1:8080");
        assert!(http("http://localhost/x").loopback);
        assert!(!http("http://example.com/x").loopback);
        assert!(matches!(
            endpoint("/srv/repo.git", Path::new("/")).unwrap(),
            Endpoint::Local(_)
        ));
        match endpoint("../up", Path::new("/base/dir")).unwrap() {
            Endpoint::Local(p) => assert_eq!(p, Path::new("/base/dir/../up")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            endpoint("ssh://example.com/a/b.git", Path::new("/")),
            Err(GitError::Unsupported(_))
        ));
    }

    #[test]
    fn credentials_go_over_plain_http_only_to_loopback() {
        assert!(matches!(
            http("https://example.com/x").auth(&One).unwrap(),
            Auth::Some(_)
        ));
        assert!(matches!(
            http("http://127.0.0.1:1/x").auth(&One).unwrap(),
            Auth::Some(_)
        ));
        let insecure = http("http://example.com/x");
        let auth = insecure.auth(&One).unwrap();
        assert!(matches!(auth, Auth::Refused));
        assert!(matches!(insecure.auth_error(&auth), GitError::Failed(m) if m.contains("https")));
        let none = insecure.auth(&NoCredentials).unwrap();
        assert!(matches!(
            insecure.auth_error(&none),
            GitError::Auth(host) if host == "example.com"
        ));
    }
}

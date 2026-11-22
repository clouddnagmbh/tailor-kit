// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The review repository of a remote URL.

use super::{ReviewHost, ReviewKind, ReviewRepo};

/// `(host, path)` of an `https://`, `http://`, `ssh://` or scp-like (`git@host:path`) URL.
/// The host keeps a `:port` for http(s) (it is the web port) and loses it for ssh (it is the
/// SSH port).
fn split(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    if let Some((scheme, rest)) = url.split_once("://") {
        let ssh = matches!(
            scheme.to_ascii_lowercase().as_str(),
            "ssh" | "git+ssh" | "git"
        );
        if !ssh && !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let hostport = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let host = if ssh {
            hostport
                .rsplit_once(':')
                .filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()))
                .map_or(hostport, |(h, _)| h)
        } else {
            hostport
        };
        return Some((host.to_ascii_lowercase(), path.to_owned()));
    }
    // scp-like: [user@]host:path (no scheme, a colon before any slash).
    let (authority, path) = url.split_once(':')?;
    if authority.contains('/') || path.starts_with("//") {
        return None;
    }
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    (!host.is_empty()).then(|| (host.to_ascii_lowercase(), path.to_owned()))
}

fn project(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let path = path.trim_matches('/');
    path.strip_suffix(".git")
        .unwrap_or(path)
        .trim_matches('/')
        .to_owned()
}

/// See [`super::detect`].
pub fn detect(remote_url: &str, hosts: &[ReviewHost]) -> Option<ReviewRepo> {
    let (host, path) = split(remote_url)?;
    let project = project(&path);
    if project.is_empty() {
        return None;
    }
    let (kind, api) = match hosts.iter().find(|h| h.host.eq_ignore_ascii_case(&host)) {
        Some(h) => (h.kind, h.api.trim().trim_end_matches('/').to_owned()),
        None => match host.as_str() {
            "github.com" => (ReviewKind::Github, "https://api.github.com".to_owned()),
            "gitlab.com" => (ReviewKind::Gitlab, "https://gitlab.com/api/v4".to_owned()),
            _ => return None,
        },
    };
    // GitHub has exactly owner/name; GitLab allows subgroups.
    if kind == ReviewKind::Github && project.split('/').count() != 2 {
        return None;
    }
    if project.split('/').any(str::is_empty) {
        return None;
    }
    Some(ReviewRepo {
        kind,
        host,
        api,
        project,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(h: &str, kind: ReviewKind, api: &str) -> ReviewHost {
        ReviewHost {
            host: h.into(),
            kind,
            api: api.into(),
        }
    }

    #[test]
    fn github_urls_in_every_form() {
        for url in [
            "https://github.com/acme/orders.git",
            "https://github.com/acme/orders",
            "https://user@github.com/acme/orders.git",
            "git@github.com:acme/orders.git",
            "ssh://git@github.com/acme/orders.git",
            "ssh://git@github.com:22/acme/orders.git",
            "https://GitHub.com/acme/orders/",
        ] {
            let r = detect(url, &[]).unwrap_or_else(|| panic!("{url}"));
            assert_eq!(r.kind, ReviewKind::Github, "{url}");
            assert_eq!(r.host, "github.com");
            assert_eq!(r.api, "https://api.github.com");
            assert_eq!(r.project, "acme/orders", "{url}");
        }
    }

    #[test]
    fn gitlab_keeps_subgroups() {
        let r = detect("git@gitlab.com:group/sub/api.git", &[]).unwrap();
        assert_eq!(r.kind, ReviewKind::Gitlab);
        assert_eq!(r.api, "https://gitlab.com/api/v4");
        assert_eq!(r.project, "group/sub/api");
        assert!(detect("https://github.com/a/b/c", &[]).is_none());
    }

    #[test]
    fn self_hosted_hosts_come_from_the_list() {
        let hosts = [
            host(
                "git.corp.example",
                ReviewKind::Github,
                "https://git.corp.example/api/v3/",
            ),
            host(
                "gitlab.corp.example:8443",
                ReviewKind::Gitlab,
                "https://gitlab.corp.example:8443/api/v4",
            ),
            host(
                "gitlab.corp.example",
                ReviewKind::Gitlab,
                "https://gitlab.corp.example/api/v4",
            ),
        ];
        let a = detect("https://git.corp.example/team/app.git", &hosts).unwrap();
        assert_eq!(
            (a.kind, a.api.as_str()),
            (ReviewKind::Github, "https://git.corp.example/api/v3")
        );
        let b = detect("https://gitlab.corp.example:8443/g/p.git", &hosts).unwrap();
        assert_eq!(b.host, "gitlab.corp.example:8443");
        assert_eq!(b.api, "https://gitlab.corp.example:8443/api/v4");
        // ssh drops the SSH port: the host entry without a port applies.
        let c = detect("ssh://git@gitlab.corp.example:2222/g/p.git", &hosts).unwrap();
        assert_eq!(c.host, "gitlab.corp.example");
        assert_eq!(c.api, "https://gitlab.corp.example/api/v4");
        assert!(detect("https://git.corp.example/team/app.git", &[]).is_none());
    }

    #[test]
    fn what_is_not_a_hosted_remote_is_none() {
        for url in [
            "/srv/repos/orders.git",
            "file:///srv/x.git",
            "../up",
            "",
            "https://example.com/a/b.git",
            "ftp://github.com/a/b",
        ] {
            assert!(detect(url, &[]).is_none(), "{url}");
        }
    }
}

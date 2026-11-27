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

/// Azure DevOps Services remote forms, as `(host for the token, API base, "org/project/repo")`:
/// `dev.azure.com/{org}/{project}/_git/{repo}`, `ssh.dev.azure.com:v3/{org}/{project}/{repo}`,
/// `{org}.visualstudio.com[/<legacy segment>]/{project}/_git/{repo}` and
/// `{org}@vs-ssh.visualstudio.com:v3/{org}/{project}/{repo}`. A project may be left out when it
/// has the repository's name (`dev.azure.com/{org}/_git/{repo}`).
fn azure_services(host: &str, path: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = path.split('/').collect();
    match host {
        "dev.azure.com" | "ssh.dev.azure.com" | "vs-ssh.visualstudio.com" => {
            let (org, project, repo) = if host == "dev.azure.com" {
                match parts.as_slice() {
                    [org, project, "_git", repo] => (*org, *project, *repo),
                    [org, "_git", repo] => (*org, *repo, *repo),
                    _ => return None,
                }
            } else {
                match parts.as_slice() {
                    ["v3", org, project, repo] => (*org, *project, *repo),
                    _ => return None,
                }
            };
            if host == "vs-ssh.visualstudio.com" {
                return Some((
                    format!("{org}.visualstudio.com"),
                    format!("https://{org}.visualstudio.com"),
                    format!("{org}/{project}/{repo}"),
                ));
            }
            Some((
                "dev.azure.com".to_owned(),
                format!("https://dev.azure.com/{org}"),
                format!("{org}/{project}/{repo}"),
            ))
        }
        h if h.ends_with(".visualstudio.com") => {
            let org = h.strip_suffix(".visualstudio.com")?;
            // Older addresses name a legacy first segment before the project.
            let rest = match parts.as_slice() {
                [_, rest @ ..] if rest.len() == 3 && rest[1] == "_git" => rest,
                rest => rest,
            };
            let (project, repo) = match rest {
                [project, "_git", repo] => (*project, *repo),
                ["_git", repo] => (*repo, *repo),
                _ => return None,
            };
            Some((
                h.to_owned(),
                format!("https://{h}"),
                format!("{org}/{project}/{repo}"),
            ))
        }
        _ => None,
    }
}

/// An Azure DevOps Server remote on a host the user listed: the path is
/// `[<api path>/]{project}/_git/{repo}`; the first part of the result is the last segment of the API path.
fn azure_server(host: &str, api: &str, path: &str) -> Option<String> {
    let api_path = api
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/'))
        .map_or("", |(_, p)| p)
        .trim_matches('/');
    let rest = if api_path.is_empty() {
        path
    } else {
        path.strip_prefix(api_path)?.trim_start_matches('/')
    };
    let scope = api_path
        .rsplit('/')
        .next()
        .filter(|c| !c.is_empty())
        .unwrap_or(host);
    let parts: Vec<&str> = rest.split('/').collect();
    let (project, repo) = match parts.as_slice() {
        [project, "_git", repo] => (*project, *repo),
        ["_git", repo] => (*repo, *repo),
        _ => return None,
    };
    Some(format!("{scope}/{project}/{repo}"))
}

/// A Bitbucket clone path: `{workspace}/{repo}` (Cloud), `scm/{PROJECT}/{repo}` (Data Center,
/// https) or `{PROJECT}/{repo}` (Data Center, `ssh://git@host:7999/PROJECT/repo.git`).
fn bitbucket_project(path: &str) -> Option<String> {
    let path = path.strip_prefix("scm/").unwrap_or(path);
    (path.split('/').count() == 2).then(|| path.to_owned())
}

/// See [`super::detect`].
pub fn detect(remote_url: &str, hosts: &[ReviewHost]) -> Option<ReviewRepo> {
    let (host, path) = split(remote_url)?;
    let project = project(&path);
    if project.is_empty() || project.split('/').any(str::is_empty) {
        return None;
    }
    let listed = hosts.iter().find(|h| h.host.eq_ignore_ascii_case(&host));
    // Azure DevOps Services names its organisation in the host or the path.
    if listed.is_none()
        && let Some((token_host, api, project)) = azure_services(&host, &project)
    {
        return Some(ReviewRepo {
            kind: ReviewKind::Azure,
            host: token_host,
            api,
            project,
        });
    }
    let (kind, api) = match listed {
        Some(h) => (h.kind, h.api.trim().trim_end_matches('/').to_owned()),
        None => match host.as_str() {
            "github.com" => (ReviewKind::Github, "https://api.github.com".to_owned()),
            "gitlab.com" => (ReviewKind::Gitlab, "https://gitlab.com/api/v4".to_owned()),
            "bitbucket.org" => (
                ReviewKind::Bitbucket,
                "https://api.bitbucket.org/2.0".to_owned(),
            ),
            "codeberg.org" => (ReviewKind::Gitea, "https://codeberg.org/api/v1".to_owned()),
            _ => return None,
        },
    };
    let project = match kind {
        // GitHub and Gitea have exactly owner/name; GitLab allows subgroups.
        ReviewKind::Github | ReviewKind::Gitea if project.split('/').count() != 2 => {
            return None;
        }
        ReviewKind::Bitbucket => bitbucket_project(&project)?,
        ReviewKind::Azure => match azure_services(&host, &project) {
            Some((_, _, p)) => p,
            None => azure_server(&host, &api, &project)?,
        },
        _ => project,
    };
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

    #[test]
    fn bitbucket_cloud_and_data_center() {
        for url in [
            "https://bitbucket.org/acme/orders.git",
            "https://user@bitbucket.org/acme/orders.git",
            "git@bitbucket.org:acme/orders.git",
            "ssh://git@bitbucket.org/acme/orders.git",
        ] {
            let r = detect(url, &[]).unwrap_or_else(|| panic!("{url}"));
            assert_eq!(r.kind, ReviewKind::Bitbucket, "{url}");
            assert_eq!(r.host, "bitbucket.org");
            assert_eq!(r.api, "https://api.bitbucket.org/2.0");
            assert_eq!(r.project, "acme/orders", "{url}");
        }
        assert!(detect("https://bitbucket.org/acme", &[]).is_none());
        let hosts = [host(
            "bb.corp.example",
            ReviewKind::Bitbucket,
            "https://bb.corp.example/rest/api/1.0",
        )];
        for url in [
            "https://bb.corp.example/scm/PROJ/api.git",
            "ssh://git@bb.corp.example:7999/PROJ/api.git",
        ] {
            let r = detect(url, &hosts).unwrap_or_else(|| panic!("{url}"));
            assert_eq!(r.kind, ReviewKind::Bitbucket, "{url}");
            assert_eq!(r.host, "bb.corp.example");
            assert_eq!(r.api, "https://bb.corp.example/rest/api/1.0");
            assert_eq!(r.project, "PROJ/api", "{url}");
        }
    }

    #[test]
    fn azure_devops_services_in_every_form() {
        for url in [
            "https://dev.azure.com/acme/shop/_git/api",
            "https://acme@dev.azure.com/acme/shop/_git/api",
            "git@ssh.dev.azure.com:v3/acme/shop/api",
            "ssh://git@ssh.dev.azure.com/v3/acme/shop/api",
        ] {
            let r = detect(url, &[]).unwrap_or_else(|| panic!("{url}"));
            assert_eq!(r.kind, ReviewKind::Azure, "{url}");
            assert_eq!(r.host, "dev.azure.com", "{url}");
            assert_eq!(r.api, "https://dev.azure.com/acme", "{url}");
            assert_eq!(r.project, "acme/shop/api", "{url}");
        }
        for url in [
            "https://acme.visualstudio.com/shop/_git/api",
            "https://acme.visualstudio.com/Legacy/shop/_git/api",
            "acme@vs-ssh.visualstudio.com:v3/acme/shop/api",
        ] {
            let r = detect(url, &[]).unwrap_or_else(|| panic!("{url}"));
            assert_eq!(r.kind, ReviewKind::Azure, "{url}");
            assert_eq!(r.host, "acme.visualstudio.com", "{url}");
            assert_eq!(r.api, "https://acme.visualstudio.com", "{url}");
            assert_eq!(r.project, "acme/shop/api", "{url}");
        }
        // The project may carry the repository's name.
        let r = detect("https://dev.azure.com/acme/_git/api", &[]).unwrap();
        assert_eq!(r.project, "acme/api/api");
        assert!(detect("https://dev.azure.com/acme/shop", &[]).is_none());
        assert!(detect("git@ssh.dev.azure.com:acme/shop/api", &[]).is_none());
    }

    #[test]
    fn azure_devops_server_comes_from_the_list() {
        let hosts = [host(
            "tfs.corp.example",
            ReviewKind::Azure,
            "https://tfs.corp.example/tfs/Main/",
        )];
        let r = detect("https://tfs.corp.example/tfs/Main/shop/_git/api", &hosts).unwrap();
        assert_eq!(r.kind, ReviewKind::Azure);
        assert_eq!(r.api, "https://tfs.corp.example/tfs/Main");
        assert_eq!(r.project, "Main/shop/api");
        assert!(detect("https://tfs.corp.example/other/shop/_git/api", &hosts).is_none());
        // Without an entry the host is unknown.
        let url = "https://tfs.corp.example/tfs/Main/shop/_git/api";
        assert!(detect(url, &[]).is_none());
    }

    #[test]
    fn gitea_codeberg_and_custom_hosts() {
        for url in [
            "https://codeberg.org/acme/orders.git",
            "git@codeberg.org:acme/orders.git",
            "ssh://git@codeberg.org/acme/orders.git",
        ] {
            let r = detect(url, &[]).unwrap_or_else(|| panic!("{url}"));
            assert_eq!(r.kind, ReviewKind::Gitea, "{url}");
            assert_eq!(r.api, "https://codeberg.org/api/v1");
            assert_eq!(r.project, "acme/orders", "{url}");
        }
        let hosts = [host(
            "git.corp.example:3000",
            ReviewKind::Gitea,
            "http://git.corp.example:3000/api/v1",
        )];
        let r = detect("http://git.corp.example:3000/acme/app.git", &hosts).unwrap();
        assert_eq!(
            (r.kind, r.project.as_str()),
            (ReviewKind::Gitea, "acme/app")
        );
        assert!(detect("https://codeberg.org/a/b/c", &[]).is_none());
    }
}

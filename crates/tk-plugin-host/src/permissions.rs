// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

//! The network permission checks of host calls. A plugin gets nothing it did not request, and
//! only what the user granted (all or nothing).

/// The entry of `network` that stands for the host of the request a hook runs for (used by
/// request-centred products; a product without such a request never grants it).
pub const REQUEST_HOST: &str = "{request}";

/// Whether `granted` contains every entry of `requested`. An update that asks for more than
/// was granted needs a new confirmation.
pub fn network_covers(granted: &[String], requested: &[String]) -> bool {
    requested
        .iter()
        .all(|entry| granted.iter().any(|g| g.eq_ignore_ascii_case(entry)))
}

/// What a plugin may use: its requested hosts if the grant covers them, else nothing.
pub fn effective_network(granted: &[String], requested: &[String]) -> Vec<String> {
    if network_covers(granted, requested) {
        requested.to_vec()
    } else {
        Vec::new()
    }
}

/// Checks one `network` entry: an exact host (optionally `host:port`), `*.example.com`
/// (subdomains only, not `example.com` itself), `{request}` or `*`.
pub fn valid_network_entry(entry: &str) -> bool {
    if entry == "*" || entry == REQUEST_HOST {
        return true;
    }
    if let Some(rest) = entry.strip_prefix('[') {
        // An IPv6 address, `[::1]` or `[::1]:8080`.
        return match rest.split_once(']') {
            Some((addr, "")) => addr.parse::<std::net::Ipv6Addr>().is_ok(),
            Some((addr, port)) => {
                addr.parse::<std::net::Ipv6Addr>().is_ok()
                    && port
                        .strip_prefix(':')
                        .is_some_and(|p| p.parse::<u16>().is_ok())
            }
            None => false,
        };
    }
    let host = entry.strip_prefix("*.").unwrap_or(entry);
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') => (name, Some(port)),
        _ => (host, None),
    };
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        && port.is_none_or(|p| p.parse::<u16>().is_ok())
}

/// Whether `network` allows a request to `url`. `request_url` is the URL of the request the
/// current hook runs for (`None` outside the `prepare` and `response` hooks).
///
/// Only `http` and `https` URLs are ever allowed. Host names compare case-insensitively; an
/// entry with a port only matches that port (default ports count).
pub fn network_allows(network: &[String], url: &str, request_url: Option<&str>) -> bool {
    let Some((host, port)) = host_and_port(url) else {
        return false;
    };
    let request = request_url.and_then(host_and_port);
    network.iter().any(|entry| {
        let entry = entry.to_ascii_lowercase();
        if entry == "*" {
            return true;
        }
        if entry == REQUEST_HOST {
            return request.as_ref() == Some(&(host.clone(), port));
        }
        let Some((pattern, want_port)) = split_entry(&entry) else {
            return false;
        };
        if want_port.is_some_and(|p| p != port) {
            return false;
        }
        match pattern.strip_prefix("*.") {
            Some(domain) => host.len() > domain.len() + 1 && host.ends_with(&format!(".{domain}")),
            None => host == pattern,
        }
    })
}

/// Host pattern and optional port of an entry (`[::1]:80` → `::1`, 80).
fn split_entry(entry: &str) -> Option<(String, Option<u16>)> {
    if let Some(rest) = entry.strip_prefix('[') {
        let (addr, tail) = rest.split_once(']')?;
        let port = match tail {
            "" => None,
            t => Some(t.strip_prefix(':')?.parse().ok()?),
        };
        return Some((addr.to_owned(), port));
    }
    match entry.rsplit_once(':') {
        Some((name, p)) => Some((name.to_owned(), Some(p.parse().ok()?))),
        None => Some((entry.to_owned(), None)),
    }
}

/// Lowercase host and port (with the scheme's default) of an `http(s)` URL.
pub fn host_and_port(url: &str) -> Option<(String, u16)> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .map(str::to_owned)
        .unwrap_or(host);
    Some((host, parsed.port_or_known_default()?))
}

/// Scheme, host and port: credentials of a request only go back to its own origin.
pub fn same_origin(a: &str, b: &str) -> bool {
    match (url::Url::parse(a), url::Url::parse(b)) {
        (Ok(a), Ok(b)) => a.origin() == b.origin() && a.origin().is_tuple(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|e| (*e).to_owned()).collect()
    }

    #[test]
    fn network_entries() {
        let api = list(&["api.example.com"]);
        assert!(network_allows(&api, "https://api.example.com/x", None));
        assert!(network_allows(&api, "http://API.Example.com:8080/", None));
        assert!(!network_allows(&api, "https://example.com/", None));
        assert!(!network_allows(
            &api,
            "https://api.example.com.evil.test/",
            None
        ));
        assert!(!network_allows(&api, "ftp://api.example.com/", None));

        let sub = list(&["*.example.com"]);
        assert!(network_allows(&sub, "https://a.b.example.com/", None));
        assert!(!network_allows(&sub, "https://example.com/", None));
        assert!(!network_allows(&sub, "https://badexample.com/", None));

        let port = list(&["localhost:8080"]);
        assert!(network_allows(&port, "http://localhost:8080/", None));
        assert!(!network_allows(&port, "http://localhost:8081/", None));
        assert!(!network_allows(&port, "http://localhost/", None));

        let any = list(&["*"]);
        assert!(network_allows(&any, "https://x.test/", None));
        assert!(!network_allows(&any, "file:///etc/passwd", None));
        assert!(!network_allows(&[], "https://x.test/", None));

        let req = list(&[REQUEST_HOST]);
        let r = Some("https://api.example.com/users");
        assert!(network_allows(&req, "https://api.example.com/meta", r));
        assert!(
            !network_allows(&req, "http://api.example.com/meta", r),
            "port 80 vs 443"
        );
        assert!(!network_allows(&req, "https://other.test/", r));
        assert!(!network_allows(&req, "https://api.example.com/", None));

        assert!(network_allows(
            &list(&["[::1]"]),
            "http://[::1]:3000/",
            None
        ));
    }

    #[test]
    fn entry_syntax() {
        for ok in [
            "*",
            "{request}",
            "a.test",
            "*.a.test",
            "localhost:8080",
            "[::1]",
            "[::1]:80",
        ] {
            assert!(valid_network_entry(ok), "{ok}");
        }
        for bad in [
            "",
            "*.",
            "a b",
            "http://a.test",
            "a.test:x",
            "a/b",
            "::1",
            "[x]",
        ] {
            assert!(!valid_network_entry(bad), "{bad}");
        }
    }

    #[test]
    fn grants_cover_requests() {
        let req = list(&["a.test"]);
        assert!(network_covers(&req, &req));
        assert!(!network_covers(&[], &req));
        let more = list(&["a.test", "b.test"]);
        assert!(!network_covers(&req, &more));
        assert!(network_covers(&more, &req));
        assert!(network_covers(&[], &[]));
        assert!(effective_network(&[], &req).is_empty());
        assert_eq!(effective_network(&more, &req), req);
        assert_eq!(effective_network(&list(&["A.TEST"]), &req), req);
    }

    #[test]
    fn origins() {
        assert!(same_origin("https://a.test/x", "https://a.test:443/y?z"));
        assert!(!same_origin("https://a.test/", "http://a.test/"));
        assert!(!same_origin("https://a.test/", "https://b.a.test/"));
    }
}

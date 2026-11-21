// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Redirect rules: resolving `Location` (RFC 3986 section 5), origin comparison, and how the
//! method changes per status code (as browsers do, Fetch spec "HTTP-redirect fetch").

/// Status codes that are followed as redirects.
pub(super) fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// What the next request looks like after a redirect with `status`.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum NextMethod {
    /// Same method, same body (307, 308, and 301/302 on anything but POST).
    Keep,
    /// Switch to GET and drop the body and its headers (303; 301/302 on POST).
    GetWithoutBody,
}

pub(super) fn next_method(status: u16, method: &str) -> NextMethod {
    match status {
        303 if !method.eq_ignore_ascii_case("GET") && !method.eq_ignore_ascii_case("HEAD") => {
            NextMethod::GetWithoutBody
        }
        301 | 302 if method.eq_ignore_ascii_case("POST") => NextMethod::GetWithoutBody,
        _ => NextMethod::Keep,
    }
}

/// Headers that describe the request body; dropped when a redirect turns the request into a GET.
pub(super) fn is_body_header(name: &str) -> bool {
    [
        "content-type",
        "content-length",
        "content-encoding",
        "content-language",
        "content-location",
        "transfer-encoding",
    ]
    .iter()
    .any(|h| name.eq_ignore_ascii_case(h))
}

/// Headers that carry credentials for the original origin; dropped on cross-origin redirects.
pub(super) fn is_credential_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("cookie")
}

struct Parts<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
}

/// Splits a URI reference into its components (RFC 3986 appendix B). The fragment is dropped.
fn split(reference: &str) -> Parts<'_> {
    let reference = reference.split('#').next().unwrap_or("");
    let mut rest = reference;
    let mut scheme = None;
    if let Some(colon) = rest.find(':') {
        let candidate = &rest[..colon];
        let valid = candidate
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if valid {
            scheme = Some(candidate);
            rest = &rest[colon + 1..];
        }
    }
    let mut authority = None;
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?']).unwrap_or(after.len());
        authority = Some(&after[..end]);
        rest = &after[end..];
    }
    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (rest, None),
    };
    Parts {
        scheme,
        authority,
        path,
        query,
    }
}

/// RFC 3986 section 5.2.4.
fn remove_dot_segments(path: &str) -> String {
    let mut input = path;
    let mut output: Vec<&str> = Vec::new();
    // Work segment by segment; a segment is "/" followed by non-slash characters.
    let absolute = input.starts_with('/');
    if absolute {
        input = &input[1..];
    }
    let segments: Vec<&str> = input.split('/').collect();
    let last = segments.len().saturating_sub(1);
    let mut trailing_slash = false;
    for (i, seg) in segments.iter().enumerate() {
        match *seg {
            "." => trailing_slash = i == last,
            ".." => {
                output.pop();
                trailing_slash = i == last;
            }
            s => {
                output.push(s);
                trailing_slash = false;
            }
        }
    }
    let mut out = String::with_capacity(path.len());
    if absolute {
        out.push('/');
    }
    out.push_str(&output.join("/"));
    if trailing_slash && !out.ends_with('/') {
        out.push('/');
    }
    out
}

/// RFC 3986 section 5.2.3.
fn merge(base_authority: bool, base_path: &str, reference_path: &str) -> String {
    if base_authority && base_path.is_empty() {
        return format!("/{reference_path}");
    }
    match base_path.rfind('/') {
        Some(i) => format!("{}{}", &base_path[..=i], reference_path),
        None => reference_path.to_owned(),
    }
}

/// Resolves a `Location` value against the URL of the request that received it.
/// Bytes that are not allowed in a URL (spaces, non-ASCII) are percent-encoded first, since
/// servers send those in practice.
pub(super) fn resolve(base: &str, location: &str) -> String {
    let location = encode_unsafe(location.trim());
    let r = split(&location);
    let b = split(base);
    let (scheme, authority, path, query);
    if let Some(s) = r.scheme {
        scheme = Some(s);
        authority = r.authority;
        path = remove_dot_segments(r.path);
        query = r.query;
    } else {
        scheme = b.scheme;
        if r.authority.is_some() {
            authority = r.authority;
            path = remove_dot_segments(r.path);
            query = r.query;
        } else {
            authority = b.authority;
            if r.path.is_empty() {
                path = b.path.to_owned();
                query = r.query.or(b.query);
            } else {
                path = if r.path.starts_with('/') {
                    remove_dot_segments(r.path)
                } else {
                    remove_dot_segments(&merge(b.authority.is_some(), b.path, r.path))
                };
                query = r.query;
            }
        }
    }
    let mut out = String::new();
    if let Some(s) = scheme {
        out.push_str(&s.to_ascii_lowercase());
        out.push(':');
    }
    if let Some(a) = authority {
        out.push_str("//");
        out.push_str(a);
    }
    out.push_str(&path);
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}

fn encode_unsafe(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_graphic() {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `true` if both URLs have the same scheme, host and port (default ports filled in).
pub(super) fn same_origin(a: &http::Uri, b: &http::Uri) -> bool {
    fn origin(u: &http::Uri) -> (String, String, Option<u16>) {
        let scheme = u.scheme_str().unwrap_or("").to_ascii_lowercase();
        let host = u.host().unwrap_or("").to_ascii_lowercase();
        let port = u.port_u16().or(match scheme.as_str() {
            "http" => Some(80),
            "https" => Some(443),
            _ => None,
        });
        (scheme, host, port)
    }
    origin(a) == origin(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_rfc3986_examples() {
        // RFC 3986 section 5.4.1 (fragments are dropped here).
        let base = "http://a/b/c/d;p?q";
        for (reference, expected) in [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q"),
            ("g#s", "http://a/b/c/g"),
            (";x", "http://a/b/c/;x"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
            ("../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            ("..g", "http://a/b/c/..g"),
            ("./../g", "http://a/b/g"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
        ] {
            assert_eq!(resolve(base, reference), expected, "{reference}");
        }
    }

    #[test]
    fn resolves_typical_locations() {
        assert_eq!(
            resolve("http://x:8080/a/b?c=1", "/redirect/1"),
            "http://x:8080/redirect/1"
        );
        assert_eq!(
            resolve("http://x/a", "https://y.test/p?q"),
            "https://y.test/p?q"
        );
        assert_eq!(resolve("http://x", "next"), "http://x/next");
        assert_eq!(resolve("http://x/a", "/a b/ü"), "http://x/a%20b/%C3%BC");
    }

    #[test]
    fn method_rules() {
        use NextMethod::*;
        assert_eq!(next_method(301, "POST"), GetWithoutBody);
        assert_eq!(next_method(302, "post"), GetWithoutBody);
        assert_eq!(next_method(301, "PUT"), Keep);
        assert_eq!(next_method(302, "GET"), Keep);
        assert_eq!(next_method(303, "POST"), GetWithoutBody);
        assert_eq!(next_method(303, "DELETE"), GetWithoutBody);
        assert_eq!(next_method(303, "GET"), Keep);
        assert_eq!(next_method(303, "HEAD"), Keep);
        assert_eq!(next_method(307, "POST"), Keep);
        assert_eq!(next_method(308, "PATCH"), Keep);
    }

    #[test]
    fn origins() {
        let u = |s: &str| s.parse::<http::Uri>().unwrap();
        assert!(same_origin(&u("http://a/x"), &u("http://A:80/y")));
        assert!(same_origin(&u("https://a/"), &u("https://a:443/")));
        assert!(!same_origin(&u("http://a/"), &u("https://a/")));
        assert!(!same_origin(&u("http://a:1/"), &u("http://a:2/")));
        assert!(!same_origin(&u("http://a/"), &u("http://b/")));
    }
}

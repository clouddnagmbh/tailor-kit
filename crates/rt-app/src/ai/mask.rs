// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Masking for the AI assistant: known secret values in all the forms they take in text
//! ([`redact`]), and credentials the app does not know as secrets (typed into a header, a
//! query parameter or a JSON body; returned by a login) by the name they sit under.

use base64::Engine as _;
use serde_json::Value;

const MASK: &str = "***";

/// See [`crate::ai::redact`].
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut forms: Vec<String> = Vec::new();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        forms.push(secret.clone());
        forms.push(secret.escape_debug().to_string());
        if let Ok(json) = serde_json::to_string(secret) {
            forms.push(json[1..json.len() - 1].to_owned());
        }
        forms.push(percent_encode(secret, false));
        forms.push(percent_encode(secret, true));
        if secret.chars().count() >= 6 {
            let b = secret.as_bytes();
            forms.push(base64::engine::general_purpose::STANDARD.encode(b));
            forms.push(base64::engine::general_purpose::STANDARD_NO_PAD.encode(b));
            forms.push(base64::engine::general_purpose::URL_SAFE.encode(b));
            forms.push(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b));
        }
    }
    forms.retain(|f| !f.is_empty() && f != MASK);
    forms.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    forms.dedup();
    let mut out = text.to_owned();
    for form in &forms {
        if out.contains(form.as_str()) {
            out = out.replace(form.as_str(), MASK);
        }
    }
    out
}

/// RFC 3986 encoding of everything but unreserved characters; `plus` encodes space as `+`.
fn percent_encode(s: &str, plus: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            b' ' if plus => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Whether a header, parameter, field or variable name names a credential.
pub fn credential_name(name: &str) -> bool {
    let n: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    const EXACT: &[&str] = &["key", "auth", "pass", "pin", "otp", "sig", "code"];
    const PARTS: &[&str] = &[
        "authorization",
        "token",
        "secret",
        "password",
        "passwd",
        "pwd",
        "apikey",
        "accesskey",
        "privatekey",
        "cookie",
        "session",
        "credential",
        "signature",
        "jwt",
        "bearer",
    ];
    EXACT.contains(&n.as_str()) || PARTS.iter().any(|p| n.contains(p))
}

/// Whether `value` holds no literal credential: only `{{variables}}`, optionally after an
/// auth scheme (`Bearer {{token}}`), or already masked.
pub fn template_only(value: &str) -> bool {
    let mut rest = value.trim();
    if let Some((scheme, after)) = rest.split_once(' ')
        && scheme.chars().all(|c| c.is_ascii_alphabetic())
    {
        rest = after.trim();
    }
    let mut stripped = String::new();
    let mut s = rest;
    while let Some(start) = s.find("{{") {
        stripped.push_str(&s[..start]);
        match s[start..].find("}}") {
            Some(end) => s = &s[start + end + 2..],
            None => {
                stripped.push_str(&s[start..]);
                s = "";
            }
        }
    }
    stripped.push_str(s);
    let stripped = stripped.trim();
    stripped.is_empty() || stripped == MASK
}

/// `value` of a credential: kept if `keep_templates` and it is only `{{variables}}`, else
/// masked (keeping an auth scheme such as `Bearer`).
pub fn credential_value(value: &str, keep_templates: bool) -> String {
    if value.is_empty() || (keep_templates && template_only(value)) {
        return value.to_owned();
    }
    match value.split_once(' ') {
        Some((scheme, _))
            if [
                "basic",
                "bearer",
                "digest",
                "token",
                "negotiate",
                "aws4-hmac-sha256",
            ]
            .contains(&scheme.to_ascii_lowercase().as_str()) =>
        {
            format!("{scheme} {MASK}")
        }
        _ => MASK.to_owned(),
    }
}

/// A header value: masked if the header names a credential.
pub fn header_value(name: &str, value: &str, keep_templates: bool) -> String {
    if credential_name(name) {
        credential_value(value, keep_templates)
    } else {
        value.to_owned()
    }
}

/// A URL with the password of its user info and the values of credential query parameters
/// masked.
pub fn url(url: &str, keep_templates: bool) -> String {
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (url, None),
    };
    let mut out = String::with_capacity(url.len());
    // scheme://user:password@host
    match base.split_once("://") {
        Some((scheme, rest)) => {
            let authority_end = rest.find('/').unwrap_or(rest.len());
            let (authority, path) = rest.split_at(authority_end);
            out.push_str(scheme);
            out.push_str("://");
            match authority.rsplit_once('@') {
                Some((info, host)) => {
                    match info.split_once(':') {
                        Some((user, pass)) => {
                            out.push_str(user);
                            out.push(':');
                            out.push_str(&credential_value(pass, keep_templates));
                        }
                        None => out.push_str(info),
                    }
                    out.push('@');
                    out.push_str(host);
                }
                None => out.push_str(authority),
            }
            out.push_str(path);
        }
        None => out.push_str(base),
    }
    if let Some(query) = query {
        out.push('?');
        let pairs: Vec<String> = query
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some((k, v)) if credential_name(k) => {
                    format!("{k}={}", credential_value(v, keep_templates))
                }
                _ => pair.to_owned(),
            })
            .collect();
        out.push_str(&pairs.join("&"));
    }
    out
}

/// JSON text with the values of credential fields masked (at any depth); other text as it
/// is, except `"name": "value"` pairs with a credential name, found by a scan (bodies cut in
/// the middle are not valid JSON).
pub fn json_text(text: &str, keep_templates: bool) -> String {
    if let Ok(mut value) = serde_json::from_str::<Value>(text) {
        if mask_value(&mut value, keep_templates) {
            return serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.to_owned());
        }
        return text.to_owned();
    }
    scan_pairs(text, keep_templates)
}

/// Masks credential fields; returns whether anything changed.
fn mask_value(value: &mut Value, keep_templates: bool) -> bool {
    let mut changed = false;
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if credential_name(k) {
                    let masked = match v {
                        Value::String(s) if keep_templates && template_only(s) => None,
                        Value::String(s) if s.is_empty() => None,
                        Value::String(_) | Value::Number(_) => Some(Value::String(MASK.into())),
                        _ => None,
                    };
                    if let Some(m) = masked {
                        *v = m;
                        changed = true;
                        continue;
                    }
                }
                changed |= mask_value(v, keep_templates);
            }
        }
        Value::Array(items) => {
            for v in items {
                changed |= mask_value(v, keep_templates);
            }
        }
        _ => {}
    }
    changed
}

/// The end (exclusive) of the JSON string starting at `start` (a `"`), if it is closed.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn scan_pairs(text: &str, keep_templates: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let Some(key_end) = string_end(bytes, i) else {
            break;
        };
        let key = &text[i + 1..key_end - 1];
        let mut j = key_end;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b':' && credential_name(key) {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' {
                // An unclosed value (cut body) is masked to the end.
                let (value, value_end) = match string_end(bytes, j) {
                    Some(end) => (&text[j + 1..end - 1], end),
                    None => (&text[j + 1..], bytes.len()),
                };
                if (!keep_templates || !template_only(value)) && !value.is_empty() {
                    out.push_str(&text[copied..j]);
                    out.push('"');
                    out.push_str(MASK);
                    out.push('"');
                    copied = value_end;
                }
                i = value_end;
                continue;
            }
        }
        i = key_end;
    }
    out.push_str(&text[copied.min(text.len())..]);
    out
}

/// `text` cut to at most `max` bytes (at a character boundary), with a note how long it was.
pub fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… [cut; {} bytes in total]", &text[..end], text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form_of_a_secret_is_replaced() {
        let secret = "p@ss w/\"rd";
        let secrets = vec![secret.to_owned()];
        let json = serde_json::to_string(secret).unwrap();
        let text = format!(
            "raw {secret} json {} pct {} plus {} b64 {} url {}",
            &json[1..json.len() - 1],
            percent_encode(secret, false),
            percent_encode(secret, true),
            base64::engine::general_purpose::STANDARD.encode(secret),
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret),
        );
        let masked = redact(&text, &secrets);
        assert_eq!(masked, "raw *** json *** pct *** plus *** b64 *** url ***");
    }

    #[test]
    fn credential_names() {
        for n in [
            "Authorization",
            "X-API-Key",
            "api_key",
            "access_token",
            "clientSecret",
            "Set-Cookie",
            "password",
            "key",
        ] {
            assert!(credential_name(n), "{n}");
        }
        for n in ["Content-Type", "name", "email", "author", "keyboard", "id"] {
            assert!(!credential_name(n), "{n}");
        }
    }

    #[test]
    fn templates_stay_literals_go() {
        assert!(template_only("{{token}}"));
        assert!(template_only("Bearer {{token}}"));
        assert!(template_only("***"));
        assert!(!template_only("Bearer abc"));
        assert!(!template_only("{{a}}x"));
        assert_eq!(credential_value("Bearer abc.def", true), "Bearer ***");
        assert_eq!(credential_value("Bearer {{t}}", true), "Bearer {{t}}");
        assert_eq!(credential_value("Bearer {{t}}", false), "Bearer ***");
        assert_eq!(credential_value("xyz", true), "***");
        assert_eq!(header_value("Accept", "xyz", false), "xyz");
    }

    #[test]
    fn urls() {
        assert_eq!(
            url("https://u:pw@h.example/a?key=abc&page=2&token={{t}}", true),
            "https://u:***@h.example/a?key=***&page=2&token={{t}}"
        );
        assert_eq!(url("{{base}}/x?api_key=1", true), "{{base}}/x?api_key=***");
        assert_eq!(url("https://h/a", false), "https://h/a");
    }

    #[test]
    fn json_bodies() {
        let masked = json_text(
            r#"{"user":"ann","password":"x1","nested":{"access_token":"t"},"list":[{"apiKey":"{{k}}"}]}"#,
            true,
        );
        let v: Value = serde_json::from_str(&masked).unwrap();
        assert_eq!(v["password"], "***");
        assert_eq!(v["nested"]["access_token"], "***");
        assert_eq!(v["list"][0]["apiKey"], "{{k}}");
        assert_eq!(v["user"], "ann");
        // Cut JSON: pairs found by the scan, an unclosed value masked to the end.
        assert_eq!(
            json_text(r#"{"a":"b","token": "xyz","c":1,"secret":"unfinish"#, false),
            r#"{"a":"b","token": "***","c":1,"secret":"***""#
        );
        assert_eq!(json_text("plain text", false), "plain text");
    }

    #[test]
    fn cutting_keeps_characters_whole() {
        assert_eq!(cut("abc", 5), "abc");
        let c = cut("äää", 3);
        assert!(c.starts_with('ä') && c.contains("6 bytes in total"), "{c}");
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The secret scanner before a commit (wave 5, strand E): finds credentials typed into
//! collection files (tokens, keys, passwords in auth, headers, URLs and bodies) so a commit
//! can refuse them with a hint ("make it a secret variable"). Both shells call [`scan`] in
//! `GitApi.commit` and refuse with `AppError::SecretsFound`; `GitApi.scan` shows the findings
//! beforehand. `rt scan` runs it in pipelines and as a Git pre-commit hook.
//!
//! The scanner works on lines, so it also reads files that are not valid YAML (a half-edited
//! file, a `.pem` next to the requests). It knows enough YAML to see which key a value belongs
//! to and which keys are siblings in the same mapping (`name: Authorization` next to
//! `value: Bearer …`). Three kinds of rules:
//!
//! 1. Token formats anywhere: GitHub (`ghp_…`, `github_pat_…`), GitLab (`glpat-`), Slack
//!    (`xox?-`), `sk-…` API keys (OpenAI, Anthropic), Stripe, AWS access key ids (`AKIA…`),
//!    Google API keys (`AIza…`), npm tokens, JWTs, PEM private keys, `Bearer`/`Basic`
//!    credentials and passwords in URLs.
//! 2. Values of keys that name a credential: `password`, `token`, `clientSecret`,
//!    `secretAccessKey`, `apiKey`, … (and query parameters with such names).
//! 3. `value` next to a `name` that names a credential (headers, parameters, variables), next
//!    to `type: apikey` (API key auth), or next to `secret: true` (a secret variable whose
//!    value ended up in the file).
//!
//! Not a finding: values that are only placeholders: `{{variables}}` (also `Bearer
//! {{token}}`), masks (`***`), environment references (`$TOKEN`, `process.env.TOKEN`),
//! `<your-token>`, booleans, and AWS's documented example keys (`…EXAMPLE`). A line that
//! contains `rt-scan:allow` (usually as a YAML comment, `# rt-scan:allow`) is skipped, and so
//! is the line after one that contains `rt-scan:allow-next-line` (for lines that cannot carry
//! a comment, e.g. inside a block scalar).
//!
//! Findings never carry the secret: the snippet masks every value found on the line.
//! Owner: wave 5, strand E.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// A credential in a file. Carries no secret: `snippet` masks the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretFinding {
    /// Path relative to the collection root.
    pub path: String,
    /// 1-based.
    pub line: u32,
    /// What was found, e.g. `"bearer token"`, `"AWS access key"`, `"password"`.
    pub kind: String,
    /// The line with the value replaced by `***`.
    pub snippet: String,
    /// How to fix it.
    pub hint: String,
}

/// A line containing this is not scanned.
pub const ALLOW: &str = "rt-scan:allow";
/// The line after one containing this is not scanned.
pub const ALLOW_NEXT_LINE: &str = "rt-scan:allow-next-line";
/// Larger files are not scanned (collection files are small; this keeps a commit of a big
/// binary-ish text file fast).
pub const MAX_SCAN_BYTES: usize = 4 * 1024 * 1024;
/// Snippets are cut to this many characters.
const SNIPPET_CHARS: usize = 160;
const MASK: &str = "***";

/// The text of a file as the shells hand it to [`scan`]: `None` for files the scanner skips
/// (not UTF-8, containing NUL bytes, larger than [`MAX_SCAN_BYTES`]).
pub fn text_of(bytes: &[u8]) -> Option<&str> {
    if bytes.len() > MAX_SCAN_BYTES || bytes.contains(&0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

/// Scans `files` (path, text). Values that are only `{{variables}}` are fine.
pub fn scan(files: &[(&str, &str)]) -> Vec<SecretFinding> {
    let mut out = Vec::new();
    for (path, text) in files {
        scan_file(path, text, &mut out);
    }
    out
}

/// One line of the error message of a refused commit (`AppError::SecretsFound`).
pub fn refusal(findings: &[SecretFinding]) -> String {
    let mut files: Vec<&str> = findings.iter().map(|f| f.path.as_str()).collect();
    files.dedup();
    let first = &findings[0];
    format!(
        "Commit refused: {} credential{} in {} file{} ({}:{} {}). {}",
        findings.len(),
        if findings.len() == 1 { "" } else { "s" },
        files.len(),
        if files.len() == 1 { "" } else { "s" },
        first.path,
        first.line,
        first.kind,
        first.hint
    )
}

// ---------------------------------------------------------------------------------------------
// Lines

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
}

/// A line as far as the scanner understands YAML (and JSON inside block scalars).
#[derive(Debug)]
struct Line<'a> {
    text: &'a str,
    /// Column where the line's content starts after any `- `; `None` for blank and comment
    /// lines.
    col: Option<usize>,
    /// Starts a sequence item (`- …`).
    item: bool,
    /// Column of the `-` of an item, else the indentation.
    dash: usize,
    /// The mapping key on this line (unquoted).
    key: Option<&'a str>,
    /// The scalar value after the key (inside quotes for quoted values).
    value: Option<Span>,
}

impl<'a> Line<'a> {
    fn value_text(&self) -> Option<&'a str> {
        self.value.map(|s| &self.text[s.start..s.end])
    }
}

fn parse_line(text: &str) -> Line<'_> {
    let text = text.strip_suffix('\r').unwrap_or(text);
    let bytes = text.as_bytes();
    let indent = bytes.iter().take_while(|b| **b == b' ').count();
    let mut line = Line {
        text,
        col: None,
        item: false,
        dash: indent,
        key: None,
        value: None,
    };
    let mut pos = indent;
    if pos >= bytes.len() || bytes[pos] == b'#' || bytes[pos] == b'\t' {
        return line;
    }
    if bytes[pos] == b'-' && (pos + 1 == bytes.len() || bytes[pos + 1] == b' ') {
        line.item = true;
        pos += 1;
        while pos < bytes.len() && bytes[pos] == b' ' {
            pos += 1;
        }
        if pos >= bytes.len() || bytes[pos] == b'#' {
            line.col = Some(pos);
            return line;
        }
    }
    line.col = Some(pos);
    let Some((key, after)) = parse_key(text, pos) else {
        return line;
    };
    line.key = Some(key);
    line.value = parse_value(text, after);
    line
}

/// A mapping key at `pos` and the position after its `:`.
fn parse_key(text: &str, pos: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    match bytes[pos] {
        q @ (b'"' | b'\'') => {
            let end = closing_quote(bytes, pos, q)?;
            let mut after = end + 1;
            while after < bytes.len() && bytes[after] == b' ' {
                after += 1;
            }
            (after < bytes.len() && bytes[after] == b':').then(|| (&text[pos + 1..end], after + 1))
        }
        b'{' | b'[' | b'|' | b'>' | b'&' | b'*' | b'!' | b'%' | b'@' | b'`' | b'#' => None,
        _ => {
            // Up to the first `:` followed by a space or the end of the line.
            let rest = &text[pos..];
            let mut at = None;
            for (i, c) in rest.char_indices() {
                if c == '#' && i > 0 && rest.as_bytes()[i - 1] == b' ' {
                    return None;
                }
                if c == ':' {
                    let next = rest.as_bytes().get(i + 1);
                    if next.is_none() || next == Some(&b' ') {
                        at = Some(i);
                        break;
                    }
                }
            }
            let at = at?;
            let key = rest[..at].trim_end();
            (!key.is_empty() && key.len() <= 100).then_some((key, pos + at + 1))
        }
    }
}

/// Index of the quote that closes the one at `open`.
fn closing_quote(bytes: &[u8], open: usize, q: u8) -> Option<usize> {
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if q == b'"' => i += 2,
            c if c == q => {
                if q == b'\'' && bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                } else {
                    return Some(i);
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// The scalar after a key: inside the quotes of a quoted value, else up to a comment, without
/// a trailing `,` (JSON). `None` for nested values (block scalars, flow collections, anchors).
fn parse_value(text: &str, after: usize) -> Option<Span> {
    let bytes = text.as_bytes();
    let mut start = after;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    if start >= bytes.len() {
        return None;
    }
    match bytes[start] {
        q @ (b'"' | b'\'') => {
            let end = closing_quote(bytes, start, q)?;
            Some(Span {
                start: start + 1,
                end,
            })
        }
        b'#' | b'|' | b'>' | b'{' | b'[' | b'&' | b'*' | b'!' => None,
        _ => {
            let rest = &text[start..];
            let end = rest.find(" #").unwrap_or(rest.len());
            let value = rest[..end].trim_end();
            let value = value.strip_suffix(',').unwrap_or(value).trim_end();
            (!value.is_empty()).then_some(Span {
                start,
                end: start + value.len(),
            })
        }
    }
}

/// The lines of the mapping that line `i` belongs to (its siblings, `i` included).
fn mapping_of(lines: &[Line<'_>], i: usize) -> Vec<usize> {
    let Some(c) = lines[i].col else {
        return Vec::new();
    };
    let mut out = vec![i];
    if !lines[i].item {
        let mut j = i;
        while j > 0 {
            j -= 1;
            match lines[j].col {
                None => {}
                Some(cj) if cj > c => {}
                Some(cj) if cj == c => {
                    out.push(j);
                    if lines[j].item {
                        break;
                    }
                }
                Some(_) => break,
            }
        }
    }
    for (j, line) in lines.iter().enumerate().skip(i + 1) {
        match line.col {
            None => {}
            Some(cj) if cj > c => {}
            Some(cj) if cj == c && !line.item => out.push(j),
            Some(_) => break,
        }
    }
    out
}

/// The key of the sequence the mapping of line `i` is an item of (`variables`, `headers`).
fn parent_key<'a>(lines: &[Line<'a>], mapping: &[usize]) -> Option<&'a str> {
    let item = *mapping.iter().find(|&&j| lines[j].item)?;
    let dash = lines[item].dash;
    lines[..item]
        .iter()
        .rev()
        .filter(|l| l.col.is_some())
        .find(|l| !(l.item && l.dash == dash) && l.col.is_some_and(|c| c <= dash))
        .and_then(|l| l.key)
}

// ---------------------------------------------------------------------------------------------
// Values

/// `{{…}}` removed.
fn without_templates(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        match rest[open..].find("}}") {
            Some(close) => rest = &rest[open + close + 2..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether `value` holds no credential of its own: empty, only variables, a mask, an
/// environment reference, an obvious placeholder.
fn is_placeholder(value: &str) -> bool {
    let rest = without_templates(value);
    let v = rest.trim();
    if v.is_empty() {
        return true;
    }
    let lower = v.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true"
            | "false"
            | "null"
            | "~"
            | "none"
            | "nil"
            | "undefined"
            | "n/a"
            | "tbd"
            | "todo"
            | "redacted"
            | "hidden"
            | "masked"
            | "\"\""
            | "''"
    ) {
        return true;
    }
    // Masks and filler: `***`, `xxxx`, `...`, `•••`.
    let first = v.chars().next().expect("not empty");
    if v.chars().count() >= 3
        && v.chars().all(|c| c == first)
        && matches!(first, '*' | 'x' | 'X' | '.' | '•' | '-' | '_')
    {
        return true;
    }
    // `<your-token>`, `[token]`.
    if (v.starts_with('<') && v.ends_with('>')) || (v.starts_with('[') && v.ends_with(']')) {
        return true;
    }
    // `$TOKEN`, `${TOKEN}`, `%TOKEN%`, `process.env.TOKEN`.
    let env_name = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    };
    if let Some(name) = v.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        return env_name(name);
    }
    if let Some(name) = v.strip_prefix('$') {
        return env_name(name);
    }
    if let Some(name) = v.strip_prefix('%').and_then(|s| s.strip_suffix('%')) {
        return env_name(name);
    }
    if let Some(name) = v.strip_prefix("process.env.") {
        return env_name(name);
    }
    lower.starts_with("your-") || lower.starts_with("your_") || lower.starts_with("your ")
}

/// What a key (or header, parameter or variable name) says its value is, if a credential.
fn credential_name(name: &str) -> Option<&'static str> {
    let n: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if n.is_empty() {
        return None;
    }
    // Tokens that are not credentials: CSRF tokens (often the literal `fetch`), paging.
    if [
        "csrf",
        "xsrf",
        "page",
        "next",
        "continuation",
        "sync",
        "cursor",
    ]
    .iter()
    .any(|x| n.contains(x))
    {
        return None;
    }
    Some(match n.as_str() {
        "authorization" | "proxyauthorization" => "authorization header",
        "pwd" | "pass" => "password",
        n if n.ends_with("secretaccesskey") => "AWS secret access key",
        n if n.ends_with("sessiontoken") && n.contains("aws") => "AWS session token",
        n if n.ends_with("clientsecret") => "client secret",
        n if n.ends_with("password") || n.ends_with("passwd") => "password",
        n if n.ends_with("passphrase") => "passphrase",
        n if n.ends_with("privatekey") => "private key",
        n if n.ends_with("apikey")
            || n.ends_with("subscriptionkey")
            || n.ends_with("functionskey")
            || n.ends_with("accesskey")
            || n.ends_with("secretkey") =>
        {
            "API key"
        }
        n if n.ends_with("token") => "token",
        n if n.ends_with("secret") => "secret",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Rules

/// A credential found on a line.
#[derive(Debug, Clone)]
struct Hit {
    span: Span,
    kind: &'static str,
    /// Suggested variable name for the hint.
    var: String,
    /// The value is a variable's own value (`variables:` list).
    variable: bool,
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The run of bytes from `start` that satisfy `ok`.
fn run(bytes: &[u8], start: usize, ok: impl Fn(u8) -> bool) -> usize {
    bytes[start..].iter().take_while(|b| ok(**b)).count()
}

struct TokenFormat {
    prefix: &'static str,
    kind: &'static str,
    var: &'static str,
    min: usize,
    max: usize,
    chars: fn(u8) -> bool,
    /// Needs both letters and digits after the prefix (keeps slugs like `sk-learn-…` out).
    mixed: bool,
}

fn alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}
fn word(b: u8) -> bool {
    is_word(b)
}
fn word_dash(b: u8) -> bool {
    is_word(b) || b == b'-'
}
fn alnum_dash(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}
fn upper_digit(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit()
}

const FORMATS: &[TokenFormat] = &[
    TokenFormat {
        prefix: "github_pat_",
        kind: "GitHub token",
        var: "githubToken",
        min: 40,
        max: 255,
        chars: word,
        mixed: false,
    },
    TokenFormat {
        prefix: "ghp_",
        kind: "GitHub token",
        var: "githubToken",
        min: 30,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "gho_",
        kind: "GitHub token",
        var: "githubToken",
        min: 30,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "ghu_",
        kind: "GitHub token",
        var: "githubToken",
        min: 30,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "ghs_",
        kind: "GitHub token",
        var: "githubToken",
        min: 30,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "ghr_",
        kind: "GitHub token",
        var: "githubToken",
        min: 30,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "glpat-",
        kind: "GitLab token",
        var: "gitlabToken",
        min: 20,
        max: 255,
        chars: word_dash,
        mixed: false,
    },
    TokenFormat {
        prefix: "xoxb-",
        kind: "Slack token",
        var: "slackToken",
        min: 10,
        max: 255,
        chars: alnum_dash,
        mixed: true,
    },
    TokenFormat {
        prefix: "xoxp-",
        kind: "Slack token",
        var: "slackToken",
        min: 10,
        max: 255,
        chars: alnum_dash,
        mixed: true,
    },
    TokenFormat {
        prefix: "xoxa-",
        kind: "Slack token",
        var: "slackToken",
        min: 10,
        max: 255,
        chars: alnum_dash,
        mixed: true,
    },
    TokenFormat {
        prefix: "xoxr-",
        kind: "Slack token",
        var: "slackToken",
        min: 10,
        max: 255,
        chars: alnum_dash,
        mixed: true,
    },
    TokenFormat {
        prefix: "xoxs-",
        kind: "Slack token",
        var: "slackToken",
        min: 10,
        max: 255,
        chars: alnum_dash,
        mixed: true,
    },
    TokenFormat {
        prefix: "xoxe-",
        kind: "Slack token",
        var: "slackToken",
        min: 10,
        max: 255,
        chars: alnum_dash,
        mixed: true,
    },
    TokenFormat {
        prefix: "sk_live_",
        kind: "Stripe key",
        var: "stripeKey",
        min: 16,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "rk_live_",
        kind: "Stripe key",
        var: "stripeKey",
        min: 16,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "sk_test_",
        kind: "Stripe key",
        var: "stripeKey",
        min: 16,
        max: 255,
        chars: alnum,
        mixed: false,
    },
    TokenFormat {
        prefix: "sk-",
        kind: "API key (sk-)",
        var: "apiKey",
        min: 20,
        max: 255,
        chars: word_dash,
        mixed: true,
    },
    TokenFormat {
        prefix: "AKIA",
        kind: "AWS access key",
        var: "awsAccessKeyId",
        min: 16,
        max: 16,
        chars: upper_digit,
        mixed: false,
    },
    TokenFormat {
        prefix: "ASIA",
        kind: "AWS access key",
        var: "awsAccessKeyId",
        min: 16,
        max: 16,
        chars: upper_digit,
        mixed: false,
    },
    TokenFormat {
        prefix: "AIza",
        kind: "Google API key",
        var: "googleApiKey",
        min: 35,
        max: 35,
        chars: word_dash,
        mixed: false,
    },
    TokenFormat {
        prefix: "npm_",
        kind: "npm token",
        var: "npmToken",
        min: 36,
        max: 36,
        chars: alnum,
        mixed: false,
    },
];

fn mixed(s: &[u8]) -> bool {
    s.iter().any(u8::is_ascii_digit) && s.iter().any(u8::is_ascii_alphabetic)
}

/// Rule 1, fixed formats.
fn token_formats(text: &str, hits: &mut Vec<Hit>) {
    let bytes = text.as_bytes();
    for f in FORMATS {
        let mut from = 0;
        while let Some(p) = text[from..].find(f.prefix) {
            let start = from + p;
            from = start + f.prefix.len();
            if start > 0 && is_word(bytes[start - 1]) {
                continue;
            }
            let body = start + f.prefix.len();
            let n = run(bytes, body, f.chars);
            if n < f.min || n > f.max {
                continue;
            }
            let end = body + n;
            if f.max == f.min && end < bytes.len() && alnum(bytes[end]) {
                continue;
            }
            if f.mixed && !mixed(&bytes[body..end]) {
                continue;
            }
            // AWS's documented example keys (`AKIAIOSFODNN7EXAMPLE`).
            if f.kind == "AWS access key" && text[start..end].ends_with("EXAMPLE") {
                continue;
            }
            hits.push(Hit {
                span: Span { start, end },
                kind: f.kind,
                var: f.var.to_owned(),
                variable: false,
            });
        }
    }
}

fn b64url(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Rule 1, JWTs: `eyJ…` `.` `eyJ…` `.` signature.
fn jwts(text: &str, hits: &mut Vec<Hit>) {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(p) = text[from..].find("eyJ") {
        let start = from + p;
        from = start + 3;
        if start > 0 && b64url(bytes[start - 1]) {
            continue;
        }
        let header = run(bytes, start, b64url);
        let mut at = start + header;
        if header < 10 || bytes.get(at) != Some(&b'.') || !text[at + 1..].starts_with("eyJ") {
            continue;
        }
        let payload = run(bytes, at + 1, b64url);
        at += 1 + payload;
        if payload < 10 || bytes.get(at) != Some(&b'.') {
            continue;
        }
        let signature = run(bytes, at + 1, b64url);
        if signature < 16 {
            continue;
        }
        let end = at + 1 + signature;
        hits.push(Hit {
            span: Span { start, end },
            kind: "JWT",
            var: "token".to_owned(),
            variable: false,
        });
        from = end;
    }
}

/// Rule 1, PEM private keys (the whole line is masked).
fn private_keys(text: &str, hits: &mut Vec<Hit>) {
    if let Some(begin) = text.find("-----BEGIN ")
        && text[begin..].contains("PRIVATE KEY")
    {
        hits.push(Hit {
            span: Span {
                start: begin,
                end: text.len(),
            },
            kind: "private key",
            var: String::new(),
            variable: false,
        });
    }
}

fn token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'+' | b'/' | b'=')
}

/// Rule 1, `Bearer <token>` and `Basic <base64 of user:password>` anywhere (also in cURL
/// commands and docs). A bearer token needs a digit or 24 characters, so prose ("Bearer
/// authentication") is not one.
fn auth_schemes(text: &str, hits: &mut Vec<Hit>) {
    let bytes = text.as_bytes();
    for (scheme, kind) in [("bearer ", "bearer token"), ("basic ", "basic credentials")] {
        let lower = text.to_ascii_lowercase();
        let mut from = 0;
        while let Some(p) = lower[from..].find(scheme) {
            let at = from + p;
            from = at + scheme.len();
            if at > 0 && is_word(bytes[at - 1]) {
                continue;
            }
            let start = at + scheme.len() + run(bytes, at + scheme.len(), |b| b == b' ');
            if text[start..].starts_with("{{") {
                continue;
            }
            let n = run(bytes, start, token_char);
            let token = &text[start..start + n];
            let found = match kind {
                "bearer token" => n >= 8 && (mixed(token.as_bytes()) || n >= 24),
                _ => basic_credentials(token),
            };
            if found && !is_placeholder(token) {
                hits.push(Hit {
                    span: Span {
                        start,
                        end: start + n,
                    },
                    kind,
                    var: if kind == "bearer token" {
                        "token"
                    } else {
                        "password"
                    }
                    .to_owned(),
                    variable: false,
                });
            }
        }
    }
}

/// Whether `s` is base64 of `user:password` with a password.
fn basic_credentials(s: &str) -> bool {
    if s.len() < 8 {
        return false;
    }
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(s) else {
        return false;
    };
    let Ok(text) = String::from_utf8(decoded) else {
        return false;
    };
    text.split_once(':').is_some_and(|(user, password)| {
        !user.chars().any(char::is_control)
            && !password.is_empty()
            && !password.chars().any(char::is_control)
            && !is_placeholder(password)
    })
}

fn url_stop(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'"' | b'\'' | b'`' | b'<' | b'>')
}

/// Rule 1 and 2 in URLs: `scheme://user:password@host` and `?token=…` query parameters.
fn urls(text: &str, hits: &mut Vec<Hit>) {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(p) = text[from..].find("://") {
        let start = from + p + 3;
        let end = start
            + run(bytes, start, |b| {
                !url_stop(b) && !matches!(b, b'/' | b'?' | b'#')
            });
        from = end.max(start);
        let authority = &text[start..end];
        if let Some(at) = authority.rfind('@')
            && let Some(colon) = authority[..at].find(':')
        {
            let password = &authority[colon + 1..at];
            if !is_placeholder(password) {
                hits.push(Hit {
                    span: Span {
                        start: start + colon + 1,
                        end: start + at,
                    },
                    kind: "password in URL",
                    var: "password".to_owned(),
                    variable: false,
                });
            }
        }
    }
    // Query parameters, also in `{{baseUrl}}/x?api_key=…`.
    let mut from = 0;
    while let Some(p) = text[from..].find(['?', '&']) {
        let at = from + p + 1;
        from = at;
        let name_len = run(bytes, at, |b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
        });
        if name_len == 0 || bytes.get(at + name_len) != Some(&b'=') {
            continue;
        }
        let name = &text[at..at + name_len];
        let Some(kind) = credential_name(name) else {
            continue;
        };
        let start = at + name_len + 1;
        let n = run(bytes, start, |b| !url_stop(b) && !matches!(b, b'&' | b'#'));
        let value = &text[start..start + n];
        if !is_placeholder(value) {
            hits.push(Hit {
                span: Span {
                    start,
                    end: start + n,
                },
                kind,
                var: name.to_owned(),
                variable: false,
            });
        }
    }
}

/// Rule 2 and 3: the value of a key that names a credential, or of `value` next to one.
fn keyed(lines: &[Line<'_>], i: usize, hits: &mut Vec<Hit>) {
    let line = &lines[i];
    let (Some(key), Some(span)) = (line.key, line.value) else {
        return;
    };
    let value = &line.text[span.start..span.end];
    let (kind, var, variable) = if let Some(kind) = credential_name(key) {
        (kind, key.to_owned(), false)
    } else if key.eq_ignore_ascii_case("value") || key.eq_ignore_ascii_case("data") {
        let mapping = mapping_of(lines, i);
        let sibling = |name: &str| {
            mapping
                .iter()
                .map(|&j| &lines[j])
                .find(|l| l.key.is_some_and(|k| k.eq_ignore_ascii_case(name)))
                .and_then(Line::value_text)
        };
        let in_variables =
            parent_key(lines, &mapping).is_some_and(|k| k == "variables" || k == "vars");
        if sibling("secret").is_some_and(|v| v == "true") {
            let name = sibling("name").unwrap_or("secret");
            ("secret variable value", name.to_owned(), true)
        } else if let Some(name) = sibling("name")
            && let Some(kind) = credential_name(name)
        {
            (kind, name.to_owned(), in_variables)
        } else if sibling("type").is_some_and(|t| t.eq_ignore_ascii_case("apikey")) {
            ("API key", "apiKey".to_owned(), false)
        } else {
            return;
        }
    } else {
        return;
    };
    if is_placeholder(value) {
        return;
    }
    let mut span = span;
    let mut kind = kind;
    if kind == "authorization header" {
        // `Bearer {{token}}` is fine; the scheme itself is not secret.
        let (scheme, rest) = value.split_once(' ').unwrap_or(("", value));
        if is_placeholder(rest) || auth_params_are_placeholders(rest) {
            return;
        }
        if !scheme.is_empty() {
            span.start += scheme.len() + 1;
            kind = match scheme.to_ascii_lowercase().as_str() {
                "bearer" => "bearer token",
                "basic" => "basic credentials",
                _ => "authorization header",
            };
        }
    }
    hits.push(Hit {
        span,
        kind,
        var: var_name(&var),
        variable,
    });
}

/// Whether the parameters of an authorization scheme (`apikey={{key}}, session="{{s}}"`) all
/// have placeholder values.
fn auth_params_are_placeholders(rest: &str) -> bool {
    let params: Vec<&str> = rest
        .split([',', ' '])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    !params.is_empty()
        && params.iter().all(|p| {
            p.split_once('=')
                .is_some_and(|(_, v)| is_placeholder(v.trim_matches(|c| c == '"' || c == '\'')))
        })
}

/// A variable name for the hint: `X-API-Key` → `xApiKey`; a single word stays as it is.
fn var_name(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    match words.as_slice() {
        [] => "secret".to_owned(),
        [one] => (*one).to_owned(),
        [first, rest @ ..] => {
            let mut out = first.to_ascii_lowercase();
            for w in rest {
                let lower = w.to_ascii_lowercase();
                let mut chars = lower.chars();
                if let Some(c) = chars.next() {
                    out.push(c.to_ascii_uppercase());
                    out.push_str(chars.as_str());
                }
            }
            out
        }
    }
}

fn hint(hit: &Hit) -> String {
    const ALLOW_HINT: &str = "Not a credential? Add `# rt-scan:allow` to the line.";
    if hit.kind == "private key" {
        return format!(
            "Do not commit private keys: keep the key file out of Git (add it to .gitignore) \
             and reference it from the client certificate settings; its passphrase belongs in \
             a secret variable. {ALLOW_HINT}"
        );
    }
    if hit.kind == "secret variable value" {
        return format!(
            "The variable `{}` is secret, so its value does not belong in the file: delete the \
             value here and set it in Environments (it is kept in the OS keychain). \
             {ALLOW_HINT}",
            hit.var
        );
    }
    if hit.variable {
        return format!(
            "Mark the variable `{}` as secret in Environments: its value then lives in the OS \
             keychain (or comes from a secret manager under External secrets) and leaves the \
             file. {ALLOW_HINT}",
            hit.var
        );
    }
    format!(
        "Make it a secret variable: in Environments add `{v}`, mark it as secret (the value is \
         kept in the OS keychain, or comes from a secret manager under External secrets) and \
         write `{{{{{v}}}}}` here instead. {ALLOW_HINT}",
        v = hit.var
    )
}

/// The line with every hit masked, trimmed and cut for display.
fn snippet(text: &str, hits: &[Hit]) -> String {
    let mut spans: Vec<Span> = hits.iter().map(|h| h.span).collect();
    spans.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));
    let mut out = String::new();
    let mut at = 0;
    for s in spans {
        if s.start < at {
            at = at.max(s.end);
            continue;
        }
        out.push_str(&text[at..s.start]);
        out.push_str(MASK);
        at = s.end;
    }
    out.push_str(&text[at.min(text.len())..]);
    let trimmed = out.trim();
    if trimmed.chars().count() > SNIPPET_CHARS {
        let cut: String = trimmed.chars().take(SNIPPET_CHARS).collect();
        format!("{cut}…")
    } else {
        trimmed.to_owned()
    }
}

fn scan_file(path: &str, text: &str, out: &mut Vec<SecretFinding>) {
    let lines: Vec<Line<'_>> = text.split('\n').map(parse_line).collect();
    let mut allow_next = false;
    for (i, line) in lines.iter().enumerate() {
        let allowed = allow_next || line.text.contains(ALLOW);
        allow_next = line.text.contains(ALLOW_NEXT_LINE);
        if allowed {
            continue;
        }
        let mut hits = Vec::new();
        token_formats(line.text, &mut hits);
        jwts(line.text, &mut hits);
        private_keys(line.text, &mut hits);
        auth_schemes(line.text, &mut hits);
        urls(line.text, &mut hits);
        keyed(&lines, i, &mut hits);
        if hits.is_empty() {
            continue;
        }
        // The most specific hit names the finding: fixed formats come first, and a hit by
        // key or scheme that overlaps a format's span adds nothing.
        let overlaps = |a: Span, b: Span| a.start < b.end && b.start < a.end;
        let first = hits[0].clone();
        let mut kinds: Vec<&str> = Vec::new();
        for h in &hits {
            let covered = !is_format(h.kind)
                && hits
                    .iter()
                    .any(|o| is_format(o.kind) && overlaps(o.span, h.span));
            if !covered && !kinds.contains(&h.kind) {
                kinds.push(h.kind);
            }
        }
        // A variable's own value: the hint says how to make that variable secret.
        let first = if first.variable || !hits.iter().any(|h| h.variable) {
            first
        } else {
            Hit {
                variable: true,
                ..hits.iter().find(|h| h.variable).expect("checked").clone()
            }
        };
        out.push(SecretFinding {
            path: path.to_owned(),
            line: u32::try_from(i + 1).unwrap_or(u32::MAX),
            kind: kinds.join(", "),
            snippet: snippet(line.text, &hits),
            hint: hint(&first),
        });
    }
}

/// Kinds found by their format, not by the key they stand under.
fn is_format(kind: &str) -> bool {
    FORMATS.iter().any(|f| f.kind == kind) || matches!(kind, "JWT" | "private key")
}

#[cfg(test)]
mod tests;

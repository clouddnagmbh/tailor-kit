// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The secret scanner before a commit (wave 5, strand E): finds credentials typed into
//! collection files (tokens, keys, passwords in auth, headers, URLs and bodies) so a commit
//! can refuse them with a hint ("make it a secret variable"). Both shells call [`scan`] in
//! `GitApi.commit` and refuse with `AppError::SecretsFound`; `GitApi.scan` shows the findings
//! beforehand. `rt scan` runs it in pipelines and as a Git pre-commit hook.
//!
//! The scanner itself (line model, token formats, credential-named keys, placeholders, masking)
//! is `tk_scan`, shared with GitTailor. This module configures it with RequestTailor's allow
//! markers, its field knowledge and its hint texts. The field knowledge is rule 3: `value` (or
//! `data`) next to a `name` that names a credential (headers, parameters, variables), next to
//! `type: apikey` (API key auth), or next to `secret: true` (a secret variable whose value
//! ended up in the file). A line that contains `rt-scan:allow` (usually as a YAML comment,
//! `# rt-scan:allow`) is skipped, and so is the line after one that contains
//! `rt-scan:allow-next-line` (for lines that cannot carry a comment, e.g. inside a block
//! scalar). Findings never carry the secret: the snippet masks every value found on the line.
//! Owner: wave 5, strand E.

use tk_scan::{Context, Extra, Hit, Rules, Scanner, credential_name};

#[cfg(test)]
use tk_scan::SNIPPET_CHARS;
pub use tk_scan::{MAX_SCAN_BYTES, refusal, text_of};

/// A credential in a file. Carries no secret: `snippet` masks the value.
pub type SecretFinding = tk_scan::Finding;

/// A line containing this is not scanned.
pub const ALLOW: &str = "rt-scan:allow";
/// The line after one containing this is not scanned.
pub const ALLOW_NEXT_LINE: &str = "rt-scan:allow-next-line";

const ALLOW_HINT: &str = "Not a credential? Add `# rt-scan:allow` to the line.";

/// RequestTailor's rules and hint texts on top of the kit scanner.
struct RequestRules;

impl Rules for RequestRules {
    /// Rule 3: `value` next to a credential `name`, `type: apikey` or `secret: true`.
    fn keyed_value(&self, ctx: &Context<'_, '_>) -> Option<Extra> {
        let key = ctx.line().key()?;
        if !(key.eq_ignore_ascii_case("value") || key.eq_ignore_ascii_case("data")) {
            return None;
        }
        let in_variables = ctx
            .parent_key()
            .is_some_and(|k| k == "variables" || k == "vars");
        if ctx.sibling("secret").is_some_and(|v| v == "true") {
            let name = ctx.sibling("name").unwrap_or("secret");
            Some(Extra {
                kind: "secret variable value",
                name: name.to_owned(),
                prefer: true,
            })
        } else if let Some(name) = ctx.sibling("name")
            && let Some(kind) = credential_name(name)
        {
            Some(Extra {
                kind,
                name: name.to_owned(),
                prefer: in_variables,
            })
        } else if ctx
            .sibling("type")
            .is_some_and(|t| t.eq_ignore_ascii_case("apikey"))
        {
            Some(Extra {
                kind: "API key",
                name: "apiKey".to_owned(),
                prefer: false,
            })
        } else {
            None
        }
    }

    fn hint(&self, hit: &Hit, _scanner: &Scanner<'_>) -> Option<String> {
        if hit.kind == "private key" {
            return Some(format!(
                "Do not commit private keys: keep the key file out of Git (add it to .gitignore) \
                 and reference it from the client certificate settings; its passphrase belongs in \
                 a secret variable. {ALLOW_HINT}"
            ));
        }
        if hit.kind == "secret variable value" {
            return Some(format!(
                "The variable `{}` is secret, so its value does not belong in the file: delete the \
                 value here and set it in Environments (it is kept in the OS keychain). \
                 {ALLOW_HINT}",
                hit.var
            ));
        }
        if hit.prefer {
            return Some(format!(
                "Mark the variable `{}` as secret in Environments: its value then lives in the OS \
                 keychain (or comes from a secret manager under External secrets) and leaves the \
                 file. {ALLOW_HINT}",
                hit.var
            ));
        }
        Some(format!(
            "Make it a secret variable: in Environments add `{v}`, mark it as secret (the value is \
             kept in the OS keychain, or comes from a secret manager under External secrets) and \
             write `{{{{{v}}}}}` here instead. {ALLOW_HINT}",
            v = hit.var
        ))
    }
}

static RULES: RequestRules = RequestRules;

fn scanner() -> Scanner<'static> {
    Scanner::new(ALLOW, ALLOW_NEXT_LINE).with_rules(&RULES)
}

/// Scans `files` (path, text). Values that are only `{{variables}}` are fine.
pub fn scan(files: &[(&str, &str)]) -> Vec<SecretFinding> {
    scanner().scan(files)
}

/// [`scan`] with the values each finding masks, in the order of the snippet's masks
/// (overlapping or touching hits are one value). Only for the secret rescue (`rt_app::rescue`), which
/// moves them into the secret store: the values never leave the Rust side.
pub fn scan_values(path: &str, text: &str) -> Vec<(SecretFinding, Vec<String>)> {
    scanner().scan_values(path, text)
}

#[cfg(test)]
mod tests;

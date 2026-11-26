// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Unit tests of the kit scanner: the line model, the generic rules, the product extension
//! point and the configurable allow markers. The product suites (`rt_app::secret_scan`) cover
//! the rules in depth.

use super::*;

#[test]
fn lines_are_parsed_like_yaml() {
    let l = parse_line("  - name: \"X-Key\" # c");
    assert!(l.item);
    assert_eq!(l.col, Some(4));
    assert_eq!(l.key, Some("name"));
    assert_eq!(l.value_text(), Some("X-Key"));
    let l = parse_line("    \"pass\": 'it''s',");
    assert_eq!(l.key, Some("pass"));
    assert_eq!(l.value_text(), Some("it''s"));
    let l = parse_line("url: https://x.example/a#frag");
    assert_eq!(l.value_text(), Some("https://x.example/a#frag"));
    assert_eq!(parse_line("data: |-").value, None);
    assert_eq!(parse_line("# comment").col, None);
    assert_eq!(parse_line("just text").key, None);
}

/// A product rule: `value` next to a `name` that names a credential.
struct NamedValue;

impl Rules for NamedValue {
    fn keyed_value(&self, ctx: &Context<'_, '_>) -> Option<Extra> {
        if !ctx.line().key()?.eq_ignore_ascii_case("value") {
            return None;
        }
        let name = ctx.sibling("name")?;
        let kind = credential_name(name)?;
        Some(Extra {
            kind,
            name: name.to_owned(),
            prefer: false,
        })
    }

    fn hint(&self, hit: &Hit, scanner: &Scanner<'_>) -> Option<String> {
        Some(format!("product hint {} {}", hit.var, scanner.allow()))
    }
}

const SIBLINGS: &str = "headers:\n  - name: Authorization\n    value: hunter2hunter2\n";

#[test]
fn sibling_rules_come_from_the_product() {
    let plain = Scanner::new("kit:allow", "kit:allow-next-line");
    assert!(plain.scan(&[("a.yml", SIBLINGS)]).is_empty());

    let rules = NamedValue;
    let with = plain.with_rules(&rules);
    let found = with.scan(&[("a.yml", SIBLINGS)]);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].line, 3);
    assert_eq!(found[0].kind, "authorization header");
    assert_eq!(found[0].snippet, "value: ***");
    assert_eq!(found[0].hint, "product hint Authorization kit:allow");
    // Placeholders stay fine for product rules too.
    let fine = "- name: Authorization\n  value: Bearer {{token}}\n";
    assert!(with.scan(&[("a.yml", fine)]).is_empty());
}

#[test]
fn allow_markers_are_the_products() {
    let scanner = Scanner::new("my-allow", "my-allow-next");
    assert!(
        scanner
            .scan(&[("a", "password: demo # my-allow\n")])
            .is_empty()
    );
    assert!(
        scanner
            .scan(&[("a", "# my-allow-next\npassword: demo\n")])
            .is_empty()
    );
    // Other products' markers mean nothing.
    assert_eq!(
        scanner
            .scan(&[("a", "password: demo # rt-scan:allow\n")])
            .len(),
        1
    );
    // Only that line.
    let two = scanner.scan(&[("a", "password: demo # my-allow\npassword: other\n")]);
    assert_eq!(two.len(), 1);
    assert_eq!(two[0].line, 2);
}

#[test]
fn generic_hints_name_the_marker() {
    let scanner = Scanner::new("my-allow", "my-allow-next");
    let f = scanner.scan(&[("a", "password: demo\n")]);
    assert!(f[0].hint.contains("`my-allow`"), "{}", f[0].hint);
    assert!(f[0].hint.contains("`password`"), "{}", f[0].hint);
    let key = ["-----BEGIN ", "PRIVATE KEY-----"].concat();
    let f = scanner.scan(&[("k.pem", &key)]);
    assert_eq!(f[0].kind, "private key");
    assert!(f[0].hint.starts_with("Do not commit private keys"));
}

#[test]
fn text_of_skips_binary_and_huge_files() {
    assert_eq!(text_of(b"a: 1\n"), Some("a: 1\n"));
    assert_eq!(text_of(b"\x00\x01PNG"), None);
    assert_eq!(text_of(&[0xff, 0xfe, b'a']), None);
    assert_eq!(text_of(&vec![b'a'; MAX_SCAN_BYTES + 1]), None);
}

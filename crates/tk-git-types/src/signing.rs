// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Commit signatures (wave 15, strand F): what the app knows about a commit's signature.
//!
//! Without a signer and a verifier the Git core neither signs nor verifies: every commit it
//! writes is unsigned, and `GitApi.verify` answers `unverified` for a signed commit and
//! `unsigned` for the rest. A distribution can add both natively through `tk_git::sign` (a
//! `CommitSigner` that signs the commit payload, a verifier that checks it).

use serde::{Deserialize, Serialize};

/// The kind of a signature, from its armor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitSignatureKind {
    Gpg,
    Ssh,
    X509,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitSignatureState {
    /// No signature.
    Unsigned,
    /// Signed, but nothing here can check it (no verifier).
    Unverified,
    /// Valid, by a key the verifier trusts.
    Good,
    /// Valid, but the key is not trusted (or not known to belong to the author).
    Untrusted,
    /// Does not match the commit.
    Bad,
    /// The key is not known.
    UnknownKey,
    /// The key or the signature expired, or the key was revoked.
    Expired,
}

/// What `GitApi.verify` says about one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSignatureCheck {
    pub oid: String,
    pub state: GitSignatureState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<GitSignatureKind>,
    /// Who signed, as the verifier reports it (name and email, or the key's principal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
    /// Key fingerprint or id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// The kind of signature an armored signature is, `None` if it is none of them.
pub fn signature_kind(armor: &str) -> Option<GitSignatureKind> {
    let first = armor.trim_start().lines().next()?;
    match first.trim() {
        "-----BEGIN PGP SIGNATURE-----" => Some(GitSignatureKind::Gpg),
        "-----BEGIN SSH SIGNATURE-----" => Some(GitSignatureKind::Ssh),
        "-----BEGIN SIGNED MESSAGE-----" | "-----BEGIN PKCS7-----" => Some(GitSignatureKind::X509),
        _ => None,
    }
}

/// The answer without a verifier: `unverified` for a signed commit, `unsigned` otherwise.
pub fn unverified(oid: &str, signature: Option<&str>) -> GitSignatureCheck {
    GitSignatureCheck {
        oid: oid.to_owned(),
        state: if signature.is_some() {
            GitSignatureState::Unverified
        } else {
            GitSignatureState::Unsigned
        },
        kind: signature.and_then(signature_kind),
        signer: None,
        key: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_kinds_from_the_armor() {
        assert_eq!(
            signature_kind("-----BEGIN PGP SIGNATURE-----\n\nabc\n"),
            Some(GitSignatureKind::Gpg)
        );
        assert_eq!(
            signature_kind("-----BEGIN SSH SIGNATURE-----\nU1NI\n"),
            Some(GitSignatureKind::Ssh)
        );
        assert_eq!(signature_kind("hello"), None);
        assert_eq!(
            unverified("a", Some("-----BEGIN SSH SIGNATURE-----")).state,
            GitSignatureState::Unverified
        );
        assert_eq!(unverified("a", None).state, GitSignatureState::Unsigned);
    }
}

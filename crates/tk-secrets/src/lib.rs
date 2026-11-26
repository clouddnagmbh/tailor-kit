// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Secret values, kept outside any file the user shares. Provenance: extracted from
//! RequestTailor's `rt-app` `secrets` module (seam K2).
//!
//! A secret is addressed by a [`SecretSlot`] (three opaque strings the product chooses) and
//! stored through a [`SecretStore`]. [`keychain::KeychainStore`] keeps values in the OS
//! keychain (native targets only).

use serde::{Deserialize, Serialize};

/// Addresses one secret value. The product decides what the parts mean; the store treats
/// them as opaque text. Not part of any wire contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretSlot {
    /// Outermost scope (may contain `/`, e.g. a canonical path).
    pub space: String,
    /// Second level within the space.
    pub group: String,
    /// The secret's own name within the group.
    pub name: String,
}

/// Why a secret store failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// The backing store (OS keychain) reported a failure.
    #[error("keychain: {0}")]
    Store(String),
    /// There is no store on this platform.
    #[error("{0}")]
    Unsupported(String),
}

pub trait SecretStore {
    fn get(&self, slot: &SecretSlot) -> Result<Option<String>, SecretError>;
    fn set(&self, slot: &SecretSlot, value: &str) -> Result<(), SecretError>;
    fn delete(&self, slot: &SecretSlot) -> Result<(), SecretError>;
}

/// OS keychain (macOS Keychain, Windows Credential Manager, Secret Service on Linux; iOS
/// Keychain). Native only.
#[cfg(not(target_arch = "wasm32"))]
pub mod keychain {
    use super::{SecretError, SecretSlot, SecretStore};

    /// Stores each value under `service` with account `<space>/<group>/<name>`.
    #[derive(Debug, Clone)]
    pub struct KeychainStore {
        pub service: String,
    }

    impl KeychainStore {
        pub fn new(service: impl Into<String>) -> Self {
            Self {
                service: service.into(),
            }
        }
    }

    /// Keychain account for one secret: `<space>/<group>/<name>`.
    ///
    /// `%` and `/` in the group and name are percent-encoded, so the last two
    /// `/`-separated segments are always group and name and the mapping is injective even
    /// though the space (a path) contains `/`. Stored secrets depend on these exact strings.
    pub fn account(slot: &SecretSlot) -> String {
        fn escape(s: &str) -> String {
            s.replace('%', "%25").replace('/', "%2F")
        }
        format!(
            "{}/{}/{}",
            slot.space,
            escape(&slot.group),
            escape(&slot.name)
        )
    }

    impl SecretStore for KeychainStore {
        fn get(&self, slot: &SecretSlot) -> Result<Option<String>, SecretError> {
            os::get(&self.service, &account(slot))
        }
        fn set(&self, slot: &SecretSlot, value: &str) -> Result<(), SecretError> {
            os::set(&self.service, &account(slot), value)
        }
        fn delete(&self, slot: &SecretSlot) -> Result<(), SecretError> {
            os::delete(&self.service, &account(slot))
        }
    }

    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "linux"
    ))]
    mod os {
        use super::SecretError;
        use keyring::{Entry, Error};

        fn entry(service: &str, account: &str) -> Result<Entry, SecretError> {
            Entry::new(service, account).map_err(store)
        }

        fn store(e: Error) -> SecretError {
            SecretError::Store(e.to_string())
        }

        pub fn get(service: &str, account: &str) -> Result<Option<String>, SecretError> {
            match entry(service, account)?.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(Error::NoEntry) => Ok(None),
                Err(e) => Err(store(e)),
            }
        }

        pub fn set(service: &str, account: &str, value: &str) -> Result<(), SecretError> {
            entry(service, account)?.set_password(value).map_err(store)
        }

        pub fn delete(service: &str, account: &str) -> Result<(), SecretError> {
            match entry(service, account)?.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => Ok(()),
                Err(e) => Err(store(e)),
            }
        }
    }

    /// Native targets without a supported keychain: refuse rather than store values elsewhere.
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "windows",
        target_os = "linux"
    )))]
    mod os {
        use super::SecretError;

        fn unsupported() -> SecretError {
            SecretError::Unsupported("no OS keychain on this platform".into())
        }

        pub fn get(_: &str, _: &str) -> Result<Option<String>, SecretError> {
            Err(unsupported())
        }

        pub fn set(_: &str, _: &str, _: &str) -> Result<(), SecretError> {
            Err(unsupported())
        }

        pub fn delete(_: &str, _: &str) -> Result<(), SecretError> {
            Err(unsupported())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn slot(space: &str, group: &str, name: &str) -> SecretSlot {
            SecretSlot {
                space: space.into(),
                group: group.into(),
                name: name.into(),
            }
        }

        #[test]
        fn account_is_space_group_name() {
            assert_eq!(
                account(&slot("abc123", "Local", "token")),
                "abc123/Local/token"
            );
        }

        #[test]
        fn account_keeps_paths_and_spaces() {
            assert_eq!(
                account(&slot("/Users/me/api", "Staging EU", "api key")),
                "/Users/me/api/Staging EU/api key"
            );
        }

        #[test]
        fn accounts_differ_per_group_and_name() {
            let a = account(&slot("c", "Local", "token"));
            assert_ne!(a, account(&slot("c", "PRD", "token")));
            assert_ne!(a, account(&slot("c", "Local", "password")));
            assert_ne!(a, account(&slot("d", "Local", "token")));
        }

        #[test]
        fn slashes_in_names_cannot_collide() {
            let a = account(&slot("/p/a", "b", "c"));
            let b = account(&slot("/p", "a/b", "c"));
            let c = account(&slot("/p", "a", "b/c"));
            assert_ne!(a, b);
            assert_ne!(a, c);
            assert_ne!(b, c);
            assert_eq!(b, "/p/a%2Fb/c");
            assert_ne!(account(&slot("/p", "a%2Fb", "c")), b);
        }

        #[test]
        fn error_texts_are_stable() {
            assert_eq!(SecretError::Store("x".into()).to_string(), "keychain: x");
            assert_eq!(SecretError::Unsupported("y".into()).to_string(), "y");
        }

        /// Talks to the real OS keychain, which CI runners do not offer. Run locally with
        /// `cargo test -p tk-secrets keychain -- --ignored`.
        #[test]
        #[ignore = "needs a real OS keychain"]
        fn real_keychain_round_trip() {
            let store = KeychainStore::new("tk-secrets-test");
            let k = slot("tk-secrets-test", "Local", "token");
            store.delete(&k).unwrap();
            assert_eq!(store.get(&k).unwrap(), None);
            store.set(&k, "s3cr3t-value").unwrap();
            assert_eq!(store.get(&k).unwrap().as_deref(), Some("s3cr3t-value"));
            store.set(&k, "changed").unwrap();
            assert_eq!(store.get(&k).unwrap().as_deref(), Some("changed"));
            store.delete(&k).unwrap();
            assert_eq!(store.get(&k).unwrap(), None);
            // Deleting a missing entry is not an error.
            store.delete(&k).unwrap();
        }
    }
}

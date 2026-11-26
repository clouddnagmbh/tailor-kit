// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! HTTPS credentials per host: what network operations ask a [`CredentialSource`] for. Where
//! a product keeps them (its secret store, the OS keychain) is the product's business.

use tk_git_types::GitError;

/// Username and password (or token) for one host.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the password, not even in test failures.
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

/// Where network operations get credentials from.
pub trait CredentialSource {
    fn get(&self, host: &str) -> Result<Option<Credentials>, GitError>;
}

/// No credentials at all.
pub struct NoCredentials;

impl CredentialSource for NoCredentials {
    fn get(&self, _host: &str) -> Result<Option<Credentials>, GitError> {
        Ok(None)
    }
}

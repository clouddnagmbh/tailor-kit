// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Git types shared by RequestTailor and GitTailor, without I/O. Moves to the `tailor-kit`
//! repository with the other `tk-*` crates; until then it lives in the RequestTailor workspace.
//!
//! Seam N7 (GitTailor implementation guide): the Git core reports [`GitError`], never a
//! product's own error type. Each product converts it into its own (RequestTailor:
//! `From<GitError> for rt_app::AppError`).
//!
//! Seam N8: refs and files of a program's own live in its [`RefNamespace`].

mod namespace;

pub use namespace::RefNamespace;

/// A failed or refused Git operation.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The operation failed or was refused; the message says why and what was being done.
    #[error("{0}")]
    Failed(String),
    /// The remote needs credentials for this host (none stored, or rejected).
    #[error("authentication required for {0}")]
    Auth(String),
    /// A commit was refused because it would add secrets; the message lists them.
    #[error("{0}")]
    SecretsFound(String),
    /// Something this Git core does not do (yet), e.g. a transport.
    #[error("{0}")]
    Unsupported(String),
    /// Reading or writing a local file outside the repository failed (settings, identity).
    #[error("{0}")]
    Io(String),
    /// An error from code the product plugs into the core (a commit signer, a credential
    /// store), carried unchanged so the product gets its own error back.
    #[error("{0}")]
    Host(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl GitError {
    /// A refused or failed operation.
    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }
}

/// Result of the Git core.
pub type GitResult<T> = Result<T, GitError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("signer said no")]
    struct HostOwn;

    #[test]
    fn messages_read_like_before() {
        assert_eq!(
            GitError::failed("cannot push: rejected").to_string(),
            "cannot push: rejected"
        );
        assert_eq!(
            GitError::Auth("example.com".into()).to_string(),
            "authentication required for example.com"
        );
    }

    #[test]
    fn a_host_error_comes_back_unchanged() {
        let e = GitError::Host(Box::new(HostOwn));
        assert_eq!(e.to_string(), "signer said no");
        let GitError::Host(inner) = e else {
            unreachable!()
        };
        assert!(inner.downcast::<HostOwn>().is_ok());
    }
}

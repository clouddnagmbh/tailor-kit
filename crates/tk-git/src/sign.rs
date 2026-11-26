// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Signing and verifying commits (wave 15 interface; strand F threads it through every commit
//! the app writes: commit, amend, merge, cherry-pick, revert, stash, `commit_to_branch`,
//! `writeCommit`).
//!
//! Without either, commits stay unsigned and `verify` answers from the commit alone
//! ([`tk_git_types::signing::unverified`]). A distribution passes its own [`Signing`] to the
//! shell (`Repo::with_signing`), e.g. one that runs `gpg` or `ssh-keygen -Y sign` with the
//! user's key. Keys and passphrases never pass through the app's own storage. Errors of a signer
//! or verifier are the product's own, carried unchanged as [`GitError::Host`].

use std::path::Path;
use std::sync::Arc;

use tk_git_types::GitError;
use tk_git_types::signing::GitSignatureCheck;

/// Signs the payload of a commit object (the commit without its `gpgsig` header), as
/// `gpg.program`/`gpg.ssh.program` would.
pub trait CommitSigner: Send + Sync {
    /// The armored signature to store in `gpgsig`, or `None` to write the commit unsigned (the
    /// signer decides from the repository's configuration, e.g. `commit.gpgsign`). An error
    /// aborts the commit (nothing is written).
    fn sign(&self, workdir: &Path, payload: &[u8]) -> Result<Option<String>, GitError>;
}

/// Checks a commit's signature against its payload.
pub trait SignatureVerifier: Send + Sync {
    fn verify(
        &self,
        workdir: &Path,
        oid: &str,
        payload: &[u8],
        signature: &str,
    ) -> Result<GitSignatureCheck, GitError>;
}

/// What a shell signs and verifies with; the default does neither.
#[derive(Clone, Default)]
pub struct Signing {
    pub signer: Option<Arc<dyn CommitSigner>>,
    pub verifier: Option<Arc<dyn SignatureVerifier>>,
}

impl std::fmt::Debug for Signing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signing")
            .field("signer", &self.signer.is_some())
            .field("verifier", &self.verifier.is_some())
            .finish()
    }
}

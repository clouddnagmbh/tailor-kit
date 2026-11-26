// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Signing through every commit writer (wave 15, strand F) and reading signatures back.
//!
//! [`Repo::with_signing`] carries a distribution's [`crate::sign::Signing`]. Every commit the
//! app writes (commit, merge, cherry-pick, revert, amend, `commit_to_branch`, `write_commit`,
//! `commit_staged`) goes through [`Repo::commit_on`] or [`Repo::write_commit_object`]: with no
//! signer these are exactly the calls the writers made before (same objects, same reflog
//! lines); with one, the payload (the commit without `gpgsig`) is handed to the signer and its
//! armor is stored in the `gpgsig` header, as `git commit -S` does. The core carries no GPG or
//! SSH code.

use gix::ObjectId;
use gix::refs::Target;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use tk_git_types::signing::{GitSignatureCheck, unverified};

use crate::repo::Repo;
use crate::util::{Context, Result, fail};

/// The raw commit split into what is signed and the signature.
pub(crate) struct SignedParts {
    pub payload: Vec<u8>,
    pub signature: Option<String>,
}

/// Splits a raw commit object: the signature of the `gpgsig` header (continuation lines
/// unfolded, ending with a line break) and the object without that header.
pub(crate) fn split_signature(raw: &[u8]) -> SignedParts {
    let header_end = raw
        .windows(2)
        .position(|w| w == b"\n\n")
        .map_or(raw.len(), |p| p + 1);
    let (header, rest) = raw.split_at(header_end);
    let mut payload = Vec::with_capacity(raw.len());
    let mut signature: Option<String> = None;
    let mut in_sig = false;
    for line in header.split_inclusive(|b| *b == b'\n') {
        if in_sig && line.starts_with(b" ") {
            let sig = signature.get_or_insert_with(String::new);
            sig.push_str(&String::from_utf8_lossy(&line[1..]));
            continue;
        }
        in_sig = false;
        if signature.is_none() && line.starts_with(b"gpgsig ") {
            in_sig = true;
            signature = Some(String::from_utf8_lossy(&line[7..]).into_owned());
            continue;
        }
        payload.extend_from_slice(line);
    }
    payload.extend_from_slice(rest);
    if let Some(sig) = &mut signature
        && !sig.ends_with('\n')
    {
        sig.push('\n');
    }
    SignedParts { payload, signature }
}

/// Whether the raw commit has a `gpgsig` header.
pub(crate) fn has_signature(commit: &gix::objs::CommitRef<'_>) -> bool {
    commit.extra_headers().find("gpgsig").is_some()
}

impl Repo {
    /// Writes `commit`, signed when a signer is set and answers with a signature; no ref moves.
    pub(crate) fn write_commit_object(&self, mut commit: gix::objs::Commit) -> Result<ObjectId> {
        if let Some(signer) = &self.signing.signer {
            let mut payload = Vec::new();
            gix::objs::WriteTo::write_to(&commit, &mut payload).ctx("cannot encode the commit")?;
            if let Some(signature) = signer.sign(&self.workdir, &payload)? {
                let signature = signature.trim_end_matches('\n');
                if signature.is_empty() {
                    return Err(fail("the signer returned an empty signature"));
                }
                commit
                    .extra_headers
                    .push(("gpgsig".into(), signature.into()));
            }
        }
        Ok(self
            .repo
            .write_object(&commit)
            .ctx("cannot write the commit")?
            .detach())
    }

    /// Commits `tree` with `parents` and moves `reference` (normally `HEAD`, which follows to
    /// its branch) to it, expecting `parents[0]` there (no commit yet: expecting none). The
    /// reflog line is the one `git commit` writes.
    pub(crate) fn commit_on(
        &self,
        reference: &str,
        committer: &gix::actor::Signature,
        author: &gix::actor::Signature,
        message: &str,
        tree: ObjectId,
        parents: Vec<ObjectId>,
    ) -> Result<ObjectId> {
        if self.signing.signer.is_none() {
            let (mut t1, mut t2) = (
                gix::date::parse::TimeBuf::default(),
                gix::date::parse::TimeBuf::default(),
            );
            return Ok(self
                .repo
                .commit_as(
                    committer.to_ref(&mut t1),
                    author.to_ref(&mut t2),
                    reference,
                    message,
                    tree,
                    parents,
                )
                .ctx("cannot commit")?
                .detach());
        }
        // As `commit_as`: the ref must still be where it is now (or still not exist).
        let current = match self
            .repo
            .try_find_reference(reference)
            .ctx("cannot read a branch")?
        {
            Some(mut r) => r.peel_to_id().ok().map(|id| id.detach()),
            None => None,
        };
        let expected = match current {
            Some(id) => PreviousValue::MustExistAndMatch(Target::Object(id)),
            None => PreviousValue::MustNotExist,
        };
        let kind = match parents.len() {
            0 => "commit (initial)",
            1 => "commit",
            _ => "commit (merge)",
        };
        let log = format!("{kind}: {}", message.lines().next().unwrap_or(""));
        let id = self.write_commit_object(gix::objs::Commit {
            tree,
            parents: parents.into(),
            author: author.clone(),
            committer: committer.clone(),
            encoding: None,
            message: message.into(),
            extra_headers: Vec::new(),
        })?;
        let name = gix::refs::FullName::try_from(reference)
            .map_err(|_| fail(format!("invalid ref {reference}")))?;
        self.repo
            .edit_reference(RefEdit {
                change: Change::Update {
                    log: LogChange {
                        mode: RefLog::AndReference,
                        force_create_reflog: false,
                        message: log.into(),
                    },
                    expected,
                    new: Target::Object(id),
                },
                name,
                deref: true,
            })
            .ctx("cannot commit")?;
        Ok(id)
    }

    /// The signature of a commit and what it signed.
    pub(crate) fn commit_signature(&self, id: ObjectId) -> Result<SignedParts> {
        let object = self.repo.find_object(id).ctx("cannot read a commit")?;
        if object.kind != gix::objs::Kind::Commit {
            return Err(fail(format!("{id} is not a commit")));
        }
        Ok(split_signature(&object.data))
    }

    /// Signature state of each commit (`GitApi.verify`): without a verifier `unverified` for
    /// a signed commit and `unsigned` for the rest.
    pub fn verify(&self, revs: &[String]) -> Result<Vec<GitSignatureCheck>> {
        let mut out = Vec::with_capacity(revs.len());
        for rev in revs {
            let id = self.resolve_rev(rev)?;
            let parts = self.commit_signature(id)?;
            let oid = id.to_string();
            out.push(match (&parts.signature, &self.signing.verifier) {
                (Some(sig), Some(verifier)) => {
                    verifier.verify(&self.workdir, &oid, &parts.payload, sig)?
                }
                (sig, _) => unverified(&oid, sig.as_deref()),
            });
        }
        Ok(out)
    }

    /// The signature header of a commit as text, for tests and tools.
    #[allow(dead_code)]
    pub(crate) fn raw_signature(&self, id: ObjectId) -> Result<Option<String>> {
        Ok(self.commit_signature(id)?.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_gpgsig_header() {
        let raw = b"tree abc\nauthor a <a> 1 +0000\ngpgsig -----BEGIN X-----\n line2\n \n -----END X-----\n\nmsg\n";
        let parts = split_signature(raw);
        assert_eq!(
            parts.payload,
            b"tree abc\nauthor a <a> 1 +0000\n\nmsg\n".to_vec()
        );
        assert_eq!(
            parts.signature.as_deref(),
            Some("-----BEGIN X-----\nline2\n\n-----END X-----\n")
        );
        let none = split_signature(b"tree abc\n\nmsg gpgsig x\n");
        assert!(none.signature.is_none());
        assert_eq!(none.payload, b"tree abc\n\nmsg gpgsig x\n".to_vec());
    }
}

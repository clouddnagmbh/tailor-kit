// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! SSH remotes (seam N10): the Git core speaks the protocol, a product brings the connection.
//!
//! `tk-git` parses `ssh://user@host:port/path` and the scp form `user@host:path` into an
//! [`SshTarget`] and asks the product's [`SshConnector`] to run `git-upload-pack` or
//! `git-receive-pack` there. The connector authenticates (agent, key files, passphrases), checks
//! the host key and hands back the byte streams of the remote command; it owns every secret, the
//! core never sees a key or a passphrase. Without a connector an SSH remote answers
//! [`GitError::Unsupported`](crate::GitError::Unsupported).

use std::io::{Read, Write};

use crate::GitError;

/// Where an SSH remote lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    /// `None`: the connector decides (`~/.ssh/config`, the current user).
    pub user: Option<String>,
    pub host: String,
    /// `None`: 22, or what the connector's configuration says.
    pub port: Option<u16>,
    /// The repository path as the URL gives it (for the scp form without the leading `:`).
    pub path: String,
}

/// The standard streams of the command running on the server.
pub struct SshStream {
    /// Standard output of the remote command.
    pub reader: Box<dyn Read + Send>,
    /// Standard input of the remote command; dropping it sends end-of-file.
    pub writer: Box<dyn Write + Send>,
}

/// Runs a command on an SSH server.
pub trait SshConnector: Send + Sync {
    /// Runs `command` (`git-upload-pack '<path>'` or `git-receive-pack '<path>'`, already quoted)
    /// on `target`. Errors: [`GitError::Auth`] with the host when the server refuses every key,
    /// [`GitError::Failed`] with a sentence for the user for everything else (unknown host
    /// rejected, network).
    fn exec(&self, target: &SshTarget, command: &str) -> Result<SshStream, GitError>;
}

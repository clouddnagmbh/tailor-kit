// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The secret scanner before a commit (wave 5, strand E): finds credentials typed into
//! collection files (tokens, keys, passwords in auth, headers, URLs and bodies) so a commit
//! can refuse them with a hint ("make it a secret variable"). Both shells call [`scan`] in
//! `GitApi.commit` and refuse with `AppError::SecretsFound`; `GitApi.scan` shows the findings
//! beforehand. `rt scan` runs it in pipelines and as a Git pre-commit hook.
//!
//! Interface commit: the types and a scanner that finds nothing. Strand E owns this file.

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

/// Scans `files` (path, text). Values that are only `{{variables}}` are fine.
pub fn scan(files: &[(&str, &str)]) -> Vec<SecretFinding> {
    let _ = files;
    Vec::new()
}

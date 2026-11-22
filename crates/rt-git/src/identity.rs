// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The app's own author identity, for when the Git configuration has none. Kept as JSON in
//! the app's data folder ([`FILE`]); the user's Git configuration is never written.

use std::path::Path;

use rt_app::AppError;
use rt_app::git::GitIdentity;

/// File name inside the app's data folder.
pub const FILE: &str = "git-identity.json";

/// The stored identity; `None` if none was set (or `data_dir` is `None`).
pub fn load(data_dir: Option<&Path>) -> Result<Option<GitIdentity>, AppError> {
    let Some(dir) = data_dir else {
        return Ok(None);
    };
    match std::fs::read(dir.join(FILE)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::Shell(format!("cannot read the Git identity: {e}"))),
    }
}

/// Stores `identity` (atomically: temporary file, then rename).
pub fn save(data_dir: Option<&Path>, identity: &GitIdentity) -> Result<(), AppError> {
    let name = identity.name.trim();
    let email = identity.email.trim();
    if name.is_empty() || email.is_empty() || name.contains(['<', '>', '\n']) || email.contains(['<', '>', '\n']) {
        return Err(AppError::Git("name and email are required (without < > or line breaks)".into()));
    }
    let Some(dir) = data_dir else {
        return Err(AppError::Shell("the app has no data folder to keep the identity in".into()));
    };
    let io = |e: std::io::Error| AppError::Shell(format!("cannot save the Git identity: {e}"));
    std::fs::create_dir_all(dir).map_err(io)?;
    let json = serde_json::to_vec_pretty(&GitIdentity {
        name: name.to_owned(),
        email: email.to_owned(),
    })
    .map_err(|e| AppError::Shell(e.to_string()))?;
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, json).map_err(io)?;
    std::fs::rename(&tmp, dir.join(FILE)).map_err(io)
}

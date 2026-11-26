// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Small shared service types. Moved from `rt_core::services` (seam N4).

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

/// A boxed future that need not be `Send` (wasm32 has no threads).
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCodePrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    /// Seconds.
    pub expires_in: u64,
}

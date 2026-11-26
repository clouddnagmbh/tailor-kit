// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Commit signatures (wave 15, strand F): what the app knows about a commit's signature.
//!
//! The builds of this repository neither sign nor verify: every commit the app writes is
//! unsigned, and `GitApi.verify` answers `unverified` for a signed commit and `unsigned` for the
//! rest. A distribution can add both natively through `rt_git::sign` (a `CommitSigner` that
//! signs the commit payload, a verifier that checks it), the same way it adds a send guard.
//!
//! The types live in the Tailor Kit (`tk_git_types::signing`); this module keeps the old paths.

pub use tk_git_types::signing::*;

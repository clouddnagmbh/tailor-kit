// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Device-code login (RFC 8628) for `github.com` and `gitlab.com`, and only if the project
//! lead registered an OAuth app: [`client_id`] is empty until then and the UI offers just the
//! token. The flow itself is `tk_review::device`; the app's client ids are here.

pub use tk_review::device::{DeviceLogin, DevicePoll, poll, start};

use super::ReviewRepo;

/// Client id of the RequestTailor OAuth app on github.com. Public by design. Empty until the
/// project lead has registered the app.
pub const GITHUB_CLIENT_ID: &str = "";
/// Client id of the RequestTailor OAuth application on gitlab.com. Empty until registered.
pub const GITLAB_CLIENT_ID: &str = "";

/// The OAuth app's client id for `repo`'s host: only `github.com` and `gitlab.com`, and only
/// once registered. `None`: device-code login is not offered.
pub fn client_id(repo: &ReviewRepo) -> Option<&'static str> {
    tk_review::device::client_id(repo, GITHUB_CLIENT_ID, GITLAB_CLIENT_ID)
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The wire boundary. The types, the `Transport` trait, the native stack (`native`) and the
//! browser's `fetch` (`fetch`) moved to `tk-net` (seam N4) and are re-exported here at their
//! old paths. The companion (a browser extension relay specific to RequestTailor) stays.

pub use tk_net::transport::*;

pub mod companion;

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Secret rescue (`rt_app::rescue`): scanning every commit with the secret scanner, and
//! rewriting the history so a found value becomes a `{{variable}}`, with backup refs first.
//!
//! Wave 12 interface placeholder; strand G implements it here.

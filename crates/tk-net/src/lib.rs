// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The network boundary shared by RequestTailor and GitTailor (seam N4).
//!
//! - [`transport`]: the wire boundary (`Transport`, `HttpRequest`, `Response`, `collect`), the
//!   native stack (`transport::native`, not on wasm32) and the browser's `fetch`
//!   (`transport::fetch`, wasm32 only).
//! - [`stream`]: the session boundary (WebSocket, Server-Sent Events, gRPC) the native stack
//!   implements, with the `text/event-stream` parser.
//! - [`services`]: `LocalBoxFuture` (a future that need not be `Send`) and `DeviceCodePrompt`.
//! - [`serde_ms`]: `Duration` as fractional milliseconds.
//!
//! Nothing here knows about requests, collections or repositories.

pub mod serde_ms;
pub mod services;
pub mod stream;
pub mod transport;

pub use services::LocalBoxFuture;

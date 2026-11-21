// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Native transport (macOS, Windows, Linux, iOS): own HTTP/1.1 and HTTP/2 stack with TLS.
//!
//! Owner: wave 2, strand A.

use super::{HttpRequest, Response, Transport, TransportError};

/// The transport used by the CLI and the desktop app. Cheap to clone; reuses connections.
#[derive(Debug, Clone)]
pub struct NativeTransport {
    _private: (),
}

impl NativeTransport {
    /// Creates a transport that trusts the operating system's certificate store.
    pub fn new() -> Result<Self, TransportError> {
        todo!("wave 2, strand A")
    }
}

impl Transport for NativeTransport {
    async fn send(&self, request: HttpRequest) -> Result<Response, TransportError> {
        let _ = request;
        todo!("wave 2, strand A")
    }
}

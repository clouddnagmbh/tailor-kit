// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Browser transport: the WHATWG `fetch` API via `web-sys`.
//!
//! Owner: wave 2, strand A.

use super::{HttpRequest, Response, Transport, TransportError};

#[derive(Debug, Clone, Default)]
pub struct FetchTransport {
    _private: (),
}

impl FetchTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Transport for FetchTransport {
    async fn send(&self, request: HttpRequest) -> Result<Response, TransportError> {
        let _ = request;
        todo!("wave 2, strand A")
    }
}

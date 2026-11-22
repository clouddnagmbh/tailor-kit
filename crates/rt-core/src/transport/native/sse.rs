// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Server-Sent Events over the native HTTP client (text/event-stream parsing). Wave 8, strand A.

use super::NativeTransport;
use crate::stream::{StreamOpen, StreamSession};
use crate::transport::TransportError;

pub(super) async fn open<'a>(
    _transport: &'a NativeTransport,
    _open: StreamOpen,
) -> Result<Box<dyn StreamSession + 'a>, TransportError> {
    Err(TransportError::Other(
        "this session type arrives in wave 8, strand A".into(),
    ))
}

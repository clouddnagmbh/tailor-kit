// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Server-Sent Events over the native HTTP client. Wave 8, strand A.
//!
//! The request goes through the same client as a send (redirects, TLS, client certificate).
//! Its timeout covers the response head only: a stream may run for hours. The body is
//! parsed by [`SseSession`]; each event is a received text message with its
//! `event:` name and `id:`. A response that is not a `2xx` `text/event-stream` ends with an
//! `error` event after a message carrying the start of its body (what the server said).

use web_time::Instant;

use super::NativeTransport;
use crate::stream::sse::SseSession;
use crate::stream::{StreamHead, StreamOpen, StreamSession};
use crate::transport::TransportError;

pub(super) async fn open<'a>(
    transport: &'a NativeTransport,
    open: StreamOpen,
) -> Result<Box<dyn StreamSession + 'a>, TransportError> {
    let StreamOpen::Sse { request } = open else {
        return Err(TransportError::Other("not an SSE session".into()));
    };
    let started = Instant::now();
    let timeout = request.options.timeout;
    let exchange = transport.send_until_head(request, None);
    let response = match timeout {
        Some(t) => tokio::time::timeout(t, exchange)
            .await
            .map_err(|_| TransportError::Timeout)??,
        None => exchange.await?,
    };
    let head = StreamHead {
        status: response.head.status,
        version: response.head.version,
        headers: response.head.headers.clone(),
        url: response.head.url.clone(),
    };
    Ok(Box::new(SseSession::new(head, response.body, started)))
}

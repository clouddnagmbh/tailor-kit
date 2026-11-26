// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The session boundary (WebSocket, Server-Sent Events, gRPC): what to open, the events a
//! session yields and the traits a transport implements. Moved from `rt_core::stream`
//! (seam N4); the preparing, masking and logging stay with the caller.
//!
//! Futures are not `Send` (wasm32).

use serde::{Deserialize, Serialize};

use crate::LocalBoxFuture;
use crate::transport::{HttpRequest, HttpVersion, TransportError};

/// The `text/event-stream` parser and a session over a response body.
pub mod sse;

/// What to open, after `prepare`. Values are resolved and include secrets: never log it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamOpen {
    /// `ws://` or `wss://`; handshake headers (auth, cookies, user headers) in order.
    WebSocket {
        url: String,
        headers: Vec<(String, String)>,
        /// `Sec-WebSocket-Protocol` values, from a user header of that name.
        protocols: Vec<String>,
        /// Connect timeout; `None` for none.
        timeout_ms: Option<u64>,
        /// Ping interval; `None` for none.
        keep_alive_ms: Option<u64>,
    },
    /// An HTTP request whose response is read as `text/event-stream`.
    Sse { request: HttpRequest },
    /// One gRPC call over HTTP/2 (`application/grpc`). `url` is `http://` or `https://`
    /// (from `grpc://`/`grpcs://`), `path` the method path `/package.Service/Method`.
    /// Messages are protobuf bytes without the 5-byte gRPC frame header.
    Grpc {
        url: String,
        path: String,
        /// Metadata (headers), including auth.
        metadata: Vec<(String, String)>,
        timeout_ms: Option<u64>,
    },
}

/// A message on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "lowercase")]
pub enum Frame {
    Text(String),
    /// Base64 in JSON.
    Binary(#[serde(with = "base64_bytes")] Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Sent,
    Received,
}

/// The response head of a session (handshake for WebSocket, headers for SSE and gRPC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamHead {
    pub status: u16,
    pub version: HttpVersion,
    pub headers: Vec<(String, String)>,
    pub url: String,
}

/// One event of a session. `at_ms`: milliseconds since the session was opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum StreamEvent {
    Opened {
        at_ms: u64,
        head: StreamHead,
    },
    Message {
        at_ms: u64,
        direction: Direction,
        frame: Frame,
        /// SSE: the `event:` field, `id:` field; gRPC: the message type name. `None` otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// The session ended. WebSocket: close code and reason; gRPC: `grpc-status` as `code`,
    /// `grpc-message` as `reason`, trailers; SSE: the server closed the stream (`code` none).
    Closed {
        at_ms: u64,
        code: Option<u32>,
        reason: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        trailers: Vec<(String, String)>,
        /// Closed by us (`close()`), not by the peer.
        by_client: bool,
    },
    Error {
        at_ms: u64,
        error: TransportError,
    },
}

/// Opens sessions. Implementations: native stack, companion relay, browser fetch (SSE).
pub trait StreamTransport {
    fn open<'a>(
        &'a self,
        open: StreamOpen,
    ) -> LocalBoxFuture<'a, Result<Box<dyn StreamSession + 'a>, TransportError>>;
}

/// An open session. `next` yields the peer's events in order (`Opened` first, `Closed` or
/// `Error` last, then `None`); sent messages are not echoed by the transport, [`Session`]
/// records them.
pub trait StreamSession {
    /// Sends one message. SSE sessions refuse (`TransportError::Other`). For gRPC, client- and
    /// bidi-streaming send any number; unary and server-streaming exactly one.
    fn send(&mut self, frame: Frame) -> LocalBoxFuture<'_, Result<(), TransportError>>;
    /// gRPC: no more messages from the client (half-close). WebSocket and SSE: no-op.
    fn finish(&mut self) -> LocalBoxFuture<'_, Result<(), TransportError>>;
    fn next(&mut self) -> LocalBoxFuture<'_, Option<StreamEvent>>;
    /// Closes the session (WebSocket close 1000, cancels SSE and gRPC).
    fn close(&mut self) -> LocalBoxFuture<'_, ()>;
}

mod base64_bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        B64.decode(text).map_err(serde::de::Error::custom)
    }
}

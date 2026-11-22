// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The wire boundary. Everything above this trait is identical on every target; below it,
//! native builds use their own HTTP stack and the WASM build uses the browser's `fetch`.
//!
//! Owner: wave 2, strand A.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use futures_core::Stream;
use serde::{Deserialize, Serialize};

/// A fully resolved request: no templates, auth applied, body encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    /// Absolute URL, already encoded.
    pub url: String,
    /// In send order. Names keep the case the user wrote.
    pub headers: Vec<(String, String)>,
    pub body: Option<HttpBody>,
    pub options: SendOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpBody {
    /// Set as `Content-Type` unless the request already has that header.
    pub content_type: Option<String>,
    pub bytes: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOptions {
    /// `None`: no timeout.
    pub timeout: Option<Duration>,
    pub follow_redirects: bool,
    pub max_redirects: u32,
    /// TLS client certificate (mTLS), chosen per host by `crate::tls::client_identity`.
    /// Transports that cannot present one (the browser's `fetch`) fail with
    /// `TransportError::Tls` instead of sending without it. Wave 5, strand D.
    pub client_identity: Option<ClientIdentity>,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            timeout: None,
            follow_redirects: true,
            max_redirects: 5,
            client_identity: None,
        }
    }
}

/// Key material of a TLS client certificate, read from the collection's files. The
/// passphrase is resolved (it is masked like a secret value wherever it could show).
#[derive(Clone, PartialEq, Eq)]
pub enum ClientIdentity {
    /// PEM certificate chain and private key (PKCS#8, PKCS#1 or SEC1; encrypted PKCS#8 with
    /// a passphrase).
    Pem {
        certificate: Bytes,
        private_key: Bytes,
        passphrase: Option<String>,
    },
    /// PKCS#12 (`.p12`, `.pfx`) archive.
    Pkcs12 { archive: Bytes, passphrase: String },
}

impl std::fmt::Debug for ClientIdentity {
    // Key material and passphrases never reach logs or error messages.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pem { .. } => f.write_str("ClientIdentity::Pem(..)"),
            Self::Pkcs12 { .. } => f.write_str("ClientIdentity::Pkcs12(..)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpVersion {
    #[serde(rename = "HTTP/1.0")]
    Http10,
    #[serde(rename = "HTTP/1.1")]
    Http11,
    #[serde(rename = "HTTP/2")]
    Http2,
    #[serde(rename = "HTTP/3")]
    Http3,
    /// The browser does not tell us.
    #[serde(rename = "unknown")]
    Unknown,
}

/// Phases of a request. Phases a transport cannot observe (e.g. DNS in the browser) are `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timing {
    #[serde(with = "crate::serde_ms::option")]
    pub dns: Option<Duration>,
    #[serde(with = "crate::serde_ms::option")]
    pub connect: Option<Duration>,
    #[serde(with = "crate::serde_ms::option")]
    pub tls: Option<Duration>,
    /// From sending the request to the first response byte (headers).
    #[serde(with = "crate::serde_ms")]
    pub ttfb: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redirect {
    pub status: u16,
    pub location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHead {
    pub status: u16,
    pub version: HttpVersion,
    pub headers: Vec<(String, String)>,
    /// URL of the final response after redirects.
    pub url: String,
    /// Redirects followed, in order. Empty in the browser (fetch hides them).
    pub redirects: Vec<Redirect>,
    pub timing: Timing,
}

/// Response body as a stream of chunks, so a 100 MB body never has to sit in memory at once.
pub type BodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>>>>;

pub struct Response {
    pub head: ResponseHead,
    pub body: BodyStream,
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "kebab-case")]
pub enum TransportError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("DNS lookup failed: {0}")]
    Dns(String),
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("TLS handshake failed: {0}")]
    Tls(String),
    #[error("request timed out")]
    Timeout,
    #[error("too many redirects (limit {0})")]
    TooManyRedirects(u32),
    /// The browser refused the request, typically CORS or mixed content. Fetch gives no detail;
    /// the UI offers the companion service here.
    #[error("blocked by the browser: {0}")]
    Blocked(String),
    #[error("{0}")]
    Other(String),
}

/// Sends one request. Implementations follow redirects and apply timeouts per `SendOptions`.
///
/// No `Send` bound on purpose: browser futures are not `Send`. Native callers that need it
/// use the concrete native transport type.
pub trait Transport {
    fn send(&self, request: HttpRequest) -> impl Future<Output = Result<Response, TransportError>>;
}

/// A response read to the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedResponse {
    pub head: ResponseHead,
    pub body: Bytes,
    /// `true` if the body was cut at the limit passed to [`collect`].
    pub truncated: bool,
    /// Total body size received (also counts bytes beyond the limit).
    pub size: u64,
    /// From sending the request until the last body byte.
    pub total: Duration,
}

/// Reads a response body to the end, keeping at most `limit` bytes in memory.
/// `started` is the instant the request was handed to the transport.
pub async fn collect(
    response: Response,
    limit: usize,
    started: web_time::Instant,
) -> Result<CompletedResponse, TransportError> {
    let Response { head, mut body } = response;
    // Chunks are kept as `Bytes` (no copy) and joined once at the end; a body that arrives in
    // one chunk is never copied.
    let mut kept: Vec<Bytes> = Vec::new();
    let mut kept_len = 0usize;
    let mut size = 0u64;
    let mut truncated = false;
    while let Some(chunk) = std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        let chunk = chunk?;
        size += chunk.len() as u64;
        let room = limit - kept_len;
        if chunk.len() <= room {
            kept_len += chunk.len();
            kept.push(chunk);
        } else {
            truncated = true;
            if room > 0 {
                kept_len += room;
                kept.push(chunk.slice(..room));
            }
        }
    }
    let body = match kept.len() {
        0 => Bytes::new(),
        1 => kept.pop().unwrap_or_default(),
        _ => {
            let mut joined = Vec::with_capacity(kept_len);
            for part in &kept {
                joined.extend_from_slice(part);
            }
            Bytes::from(joined)
        }
    };
    Ok(CompletedResponse {
        head,
        body,
        truncated,
        size,
        total: started.elapsed(),
    })
}

pub mod companion;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(target_arch = "wasm32")]
pub mod fetch;

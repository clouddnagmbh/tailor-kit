// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! gRPC over HTTP/2 (application/grpc framing, trailers, streaming). Wave 8, strand B.
//!
//! - One HTTP/2 connection per call: prior knowledge for `http://`, ALPN `h2` for `https://`
//!   (our connector and TLS setup, so the OS trust store applies). A server that does not
//!   negotiate HTTP/2 is an error at open.
//! - Request: `POST {path}`, `content-type: application/grpc`, `te: trailers`, the metadata
//!   in order, `grpc-timeout` from the timeout. The body is a stream of 5-byte
//!   length-prefixed messages (compression flag 0) fed by [`StreamSession::send`];
//!   [`StreamSession::finish`] ends it (half-close).
//! - Response: the head is [`StreamEvent::Opened`]; messages are unframed; `grpc-status` and
//!   `grpc-message` come from the trailers or, for a trailers-only response, the head. A
//!   non-200 HTTP status maps to a gRPC code as the gRPC spec says. Compressed messages
//!   (`grpc-encoding` other than `identity`, or the compression flag) are refused with a
//!   clear error: we never offer compression.
//! - [`StreamSession::close`] cancels the call (the request body fails, so the stream is
//!   reset) and yields `Closed { by_client: true }`.
//! - The timeout (`grpc-timeout`) also bounds the client side: past it the call is cancelled
//!   with `DEADLINE_EXCEEDED` (4).

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use http::header::{HeaderName, HeaderValue};
use http_body_util::BodyExt as _;
use hyper::body::{Body, Incoming};
use hyper_util::client::legacy::connect::Connection as _;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use tower_service::Service as _;
use web_time::Instant;

use super::NativeTransport;
use super::connector::Connector;
use crate::services::LocalBoxFuture;
use crate::stream::{Direction, Frame, StreamEvent, StreamHead, StreamOpen, StreamSession};
use crate::transport::{HttpVersion, TransportError};

/// Larger messages are refused (the gRPC default limit is 4 MiB; we allow more).
const MAX_MESSAGE: usize = 64 * 1024 * 1024;
/// gRPC status codes used here.
const CANCELLED: u32 = 1;
const UNKNOWN: u32 = 2;
const DEADLINE_EXCEEDED: u32 = 4;
const INTERNAL: u32 = 13;

pub(super) async fn open<'a>(
    transport: &'a NativeTransport,
    open: StreamOpen,
) -> Result<Box<dyn StreamSession + 'a>, TransportError> {
    let StreamOpen::Grpc {
        url,
        path,
        metadata,
        timeout_ms,
    } = open
    else {
        return Err(TransportError::Other("not a gRPC call".into()));
    };
    let started = Instant::now();
    let deadline = timeout_ms.filter(|ms| *ms > 0).and_then(|ms| {
        tokio::time::Instant::now().checked_add(std::time::Duration::from_millis(ms))
    });
    let full_url = format!("{}{}", url.trim_end_matches('/'), path);
    let uri = super::parse_url(&full_url)?;

    let connect = async {
        let mut connector = Connector::new(transport.tls.clone(), transport.seen.clone());
        let conn = connector.call(uri.clone()).await?;
        if !conn.connected().is_negotiated_h2() && uri.scheme_str() == Some("https") {
            return Err(TransportError::Other(format!(
                "{url}: the server does not speak HTTP/2 (ALPN h2), which gRPC needs"
            )));
        }
        let (sender, connection) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
            .timer(TokioTimer::new())
            .handshake::<_, Outgoing>(conn)
            .await
            .map_err(|e| TransportError::Connect(format!("{url}: HTTP/2: {}", super::chain(&e))))?;
        Ok::<_, TransportError>((sender, connection))
    };
    let (mut sender, connection) = match deadline {
        Some(d) => tokio::time::timeout_at(d, connect)
            .await
            .map_err(|_| TransportError::Timeout)??,
        None => connect.await?,
    };
    let driver = tokio::spawn(async move {
        let _ = connection.await;
    });

    let outbox = Arc::new(Mutex::new(Outbox::default()));
    let request = build_request(&uri, &metadata, timeout_ms, Outgoing(outbox.clone()))?;
    let response: ResponseFuture = Box::pin(sender.send_request(request));
    Ok(Box::new(GrpcSession {
        outbox,
        state: State::Waiting(response),
        _sender: sender,
        driver,
        started,
        deadline,
        url: full_url,
        buffer: Vec::new(),
        events: VecDeque::new(),
        done: false,
    }))
}

fn build_request(
    uri: &http::Uri,
    metadata: &[(String, String)],
    timeout_ms: Option<u64>,
    body: Outgoing,
) -> Result<http::Request<Outgoing>, TransportError> {
    let mut req = http::Request::new(body);
    *req.method_mut() = http::Method::POST;
    *req.uri_mut() = uri.clone();
    *req.version_mut() = http::Version::HTTP_2;
    let headers = req.headers_mut();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/grpc"),
    );
    headers.insert(http::header::TE, HeaderValue::from_static("trailers"));
    headers.insert(
        http::header::USER_AGENT,
        HeaderValue::from_static(concat!("requesttailor-grpc/", env!("CARGO_PKG_VERSION"))),
    );
    if let Some(ms) = timeout_ms.filter(|ms| *ms > 0)
        && let Ok(v) = HeaderValue::from_str(&grpc_timeout(ms))
    {
        headers.insert("grpc-timeout", v);
    }
    for (name, value) in metadata {
        let lower = name.to_ascii_lowercase();
        // The protocol's own headers are ours; HTTP/2 forbids connection headers.
        if matches!(
            lower.as_str(),
            "content-type"
                | "te"
                | "content-length"
                | "connection"
                | "transfer-encoding"
                | "host"
                | "grpc-timeout"
                | "grpc-encoding"
                | "grpc-accept-encoding"
        ) || lower.starts_with(':')
        {
            continue;
        }
        let header_name = HeaderName::from_bytes(lower.as_bytes())
            .map_err(|_| TransportError::Other(format!("invalid metadata name: {name:?}")))?;
        let header_value = HeaderValue::from_bytes(value.as_bytes()).map_err(|_| {
            TransportError::Other(format!("invalid value for metadata {name}: {value:?}"))
        })?;
        headers.append(header_name, header_value);
    }
    Ok(req)
}

/// `grpc-timeout`: at most eight digits, so long timeouts go in seconds.
fn grpc_timeout(ms: u64) -> String {
    if ms < 100_000_000 {
        format!("{ms}m")
    } else {
        format!("{}S", (ms / 1000).min(99_999_999))
    }
}

type ResponseFuture =
    Pin<Box<dyn Future<Output = Result<http::Response<Incoming>, hyper::Error>> + Send>>;

/// Messages waiting for the request body.
#[derive(Default)]
struct Outbox {
    queue: VecDeque<Bytes>,
    finished: bool,
    cancelled: bool,
    waker: Option<Waker>,
}

/// The request body: what `send` queued, until `finish` (or an error on `close`, which
/// resets the stream).
struct Outgoing(Arc<Mutex<Outbox>>);

impl Body for Outgoing {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Bytes>, io::Error>>> {
        let mut outbox = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if outbox.cancelled {
            return Poll::Ready(Some(Err(io::Error::other("cancelled by the client"))));
        }
        if let Some(bytes) = outbox.queue.pop_front() {
            return Poll::Ready(Some(Ok(hyper::body::Frame::data(bytes))));
        }
        if outbox.finished {
            return Poll::Ready(None);
        }
        outbox.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

enum State {
    Waiting(ResponseFuture),
    Body(Incoming),
    Done,
}

struct GrpcSession {
    outbox: Arc<Mutex<Outbox>>,
    state: State,
    /// Keeps the connection open while the call runs.
    _sender: hyper::client::conn::http2::SendRequest<Outgoing>,
    driver: tokio::task::JoinHandle<()>,
    started: Instant,
    deadline: Option<tokio::time::Instant>,
    url: String,
    buffer: Vec<u8>,
    events: VecDeque<StreamEvent>,
    done: bool,
}

impl Drop for GrpcSession {
    fn drop(&mut self) {
        self.driver.abort();
    }
}

enum Step {
    Head(Result<http::Response<Incoming>, hyper::Error>),
    Frame(Option<Result<hyper::body::Frame<Bytes>, hyper::Error>>),
    Deadline,
}

impl GrpcSession {
    fn at_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn with_outbox(&self, f: impl FnOnce(&mut Outbox)) {
        let mut outbox = self.outbox.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut outbox);
        if let Some(waker) = outbox.waker.take() {
            waker.wake();
        }
    }

    /// Ends the call with `event`; the request body is cancelled if still open.
    fn end(&mut self, event: StreamEvent) {
        self.events.push_back(event);
        self.done = true;
        self.state = State::Done;
        self.with_outbox(|o| {
            if !o.finished {
                o.cancelled = true;
            }
        });
    }

    fn error(&mut self, message: String) {
        let at_ms = self.at_ms();
        self.end(StreamEvent::Error {
            at_ms,
            error: TransportError::Other(message),
        });
    }

    fn closed(&mut self, code: u32, reason: String, trailers: Vec<(String, String)>) {
        let at_ms = self.at_ms();
        self.end(StreamEvent::Closed {
            at_ms,
            code: Some(code),
            reason,
            trailers,
            by_client: false,
        });
    }

    fn on_head(&mut self, response: http::Response<Incoming>) {
        let status = response.status().as_u16();
        let headers = header_list(response.headers());
        let at_ms = self.at_ms();
        self.events.push_back(StreamEvent::Opened {
            at_ms,
            head: StreamHead {
                status,
                version: HttpVersion::Http2,
                headers: headers.clone(),
                url: self.url.clone(),
            },
        });
        let grpc_status = status_of(&headers);
        if status != 200 {
            let (code, reason) = grpc_status.unwrap_or_else(|| {
                (
                    http_to_grpc(status),
                    format!("HTTP {status} instead of a gRPC response"),
                )
            });
            self.closed(code, reason, Vec::new());
            return;
        }
        let content_type = header(&headers, "content-type").unwrap_or_default();
        if !content_type.starts_with("application/grpc") {
            // A trailers-only answer may omit it; anything else is not gRPC.
            if grpc_status.is_none() {
                self.error(format!(
                    "not a gRPC response (content-type: {})",
                    if content_type.is_empty() {
                        "none"
                    } else {
                        content_type
                    }
                ));
                return;
            }
        }
        if let Some(encoding) = header(&headers, "grpc-encoding")
            && !encoding.eq_ignore_ascii_case("identity")
        {
            self.error(format!(
                "the server compressed its messages (grpc-encoding: {encoding}); \
                 compressed gRPC responses are not supported"
            ));
            return;
        }
        if let Some((code, reason)) = grpc_status {
            // Trailers-only: the status is in the head and the body is empty.
            self.closed(code, reason, other_trailers(&headers));
            return;
        }
        self.state = State::Body(response.into_body());
    }

    fn on_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        while self.buffer.len() >= 5 {
            let compressed = self.buffer[0];
            let len = u32::from_be_bytes([
                self.buffer[1],
                self.buffer[2],
                self.buffer[3],
                self.buffer[4],
            ]) as usize;
            if compressed != 0 {
                self.error(
                    "the server sent a compressed message; compressed gRPC responses are \
                     not supported"
                        .into(),
                );
                return;
            }
            if len > MAX_MESSAGE {
                self.error(format!(
                    "the server sent a message of {len} bytes (limit {MAX_MESSAGE})"
                ));
                return;
            }
            if self.buffer.len() < 5 + len {
                return;
            }
            let message: Vec<u8> = self.buffer.drain(..5 + len).skip(5).collect();
            let at_ms = self.at_ms();
            self.events.push_back(StreamEvent::Message {
                at_ms,
                direction: Direction::Received,
                frame: Frame::Binary(message),
                name: None,
                id: None,
            });
        }
    }

    fn on_frame(&mut self, frame: Option<Result<hyper::body::Frame<Bytes>, hyper::Error>>) {
        match frame {
            Some(Ok(frame)) => match frame.into_data() {
                Ok(data) => self.on_data(&data),
                Err(frame) => {
                    if let Ok(trailers) = frame.into_trailers() {
                        let trailers = header_list(&trailers);
                        if !self.buffer.is_empty() {
                            self.error("the server ended inside a message".into());
                            return;
                        }
                        let (code, reason) = status_of(&trailers).unwrap_or((
                            UNKNOWN,
                            "the server sent trailers without grpc-status".into(),
                        ));
                        self.closed(code, reason, other_trailers(&trailers));
                    }
                }
            },
            Some(Err(e)) => {
                let message = super::chain(&e);
                self.closed(
                    INTERNAL,
                    format!("the stream failed: {message}"),
                    Vec::new(),
                );
            }
            None => self.closed(
                INTERNAL,
                "the server ended the stream without a status".into(),
                Vec::new(),
            ),
        }
    }

    async fn step(&mut self) -> Step {
        let deadline = self.deadline;
        let wait = async {
            match &mut self.state {
                State::Waiting(response) => Step::Head(response.await),
                State::Body(body) => Step::Frame(body.frame().await),
                State::Done => std::future::pending().await,
            }
        };
        match deadline {
            Some(d) => tokio::time::timeout_at(d, wait)
                .await
                .unwrap_or(Step::Deadline),
            None => wait.await,
        }
    }
}

impl StreamSession for GrpcSession {
    fn send(&mut self, frame: Frame) -> LocalBoxFuture<'_, Result<(), TransportError>> {
        Box::pin(async move {
            let Frame::Binary(message) = frame else {
                return Err(TransportError::Other(
                    "gRPC messages are protobuf bytes, not text".into(),
                ));
            };
            if self.done {
                return Err(TransportError::Other("the call has ended".into()));
            }
            let len = u32::try_from(message.len())
                .ok()
                .filter(|l| (*l as usize) <= MAX_MESSAGE)
                .ok_or_else(|| TransportError::Other("message too large".into()))?;
            let mut framed = Vec::with_capacity(5 + message.len());
            framed.push(0);
            framed.extend_from_slice(&len.to_be_bytes());
            framed.extend_from_slice(&message);
            let mut result = Ok(());
            self.with_outbox(|o| {
                if o.finished {
                    result = Err(TransportError::Other(
                        "no more messages: the client side is closed".into(),
                    ));
                } else {
                    o.queue.push_back(Bytes::from(framed));
                }
            });
            result
        })
    }

    fn finish(&mut self) -> LocalBoxFuture<'_, Result<(), TransportError>> {
        Box::pin(async move {
            self.with_outbox(|o| o.finished = true);
            Ok(())
        })
    }

    fn next(&mut self) -> LocalBoxFuture<'_, Option<StreamEvent>> {
        Box::pin(async move {
            loop {
                if let Some(event) = self.events.pop_front() {
                    return Some(event);
                }
                if self.done {
                    return None;
                }
                match self.step().await {
                    Step::Head(Ok(response)) => self.on_head(response),
                    Step::Head(Err(e)) => {
                        let at_ms = self.at_ms();
                        let error = if e.is_timeout() {
                            TransportError::Timeout
                        } else {
                            TransportError::Other(super::chain(&e))
                        };
                        self.end(StreamEvent::Error { at_ms, error });
                    }
                    Step::Frame(frame) => self.on_frame(frame),
                    Step::Deadline => {
                        let at_ms = self.at_ms();
                        self.end(StreamEvent::Closed {
                            at_ms,
                            code: Some(DEADLINE_EXCEEDED),
                            reason: "deadline exceeded".into(),
                            trailers: Vec::new(),
                            by_client: true,
                        });
                    }
                }
            }
        })
    }

    fn close(&mut self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            if self.done {
                return;
            }
            self.with_outbox(|o| o.cancelled = true);
            let at_ms = self.at_ms();
            // Dropping the response (body) resets the stream.
            self.end(StreamEvent::Closed {
                at_ms,
                code: Some(CANCELLED),
                reason: "cancelled by the client".into(),
                trailers: Vec::new(),
                by_client: true,
            });
        })
    }
}

fn header_list(map: &http::HeaderMap) -> Vec<(String, String)> {
    map.iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect()
}

fn header<'h>(headers: &'h [(String, String)], name: &str) -> Option<&'h str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// `grpc-status` and the percent-decoded `grpc-message`.
fn status_of(headers: &[(String, String)]) -> Option<(u32, String)> {
    let code = header(headers, "grpc-status")?.trim().parse().ok()?;
    let message = header(headers, "grpc-message").map_or_else(String::new, percent_decode);
    Some((code, message))
}

fn other_trailers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .filter(|(k, _)| {
            !matches!(
                k.as_str(),
                "grpc-status" | "grpc-message" | "content-type" | "grpc-status-details-bin"
            )
        })
        .cloned()
        .collect()
}

fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

/// The gRPC code for an HTTP status that is not 200 (gRPC "HTTP to gRPC status code
/// mapping").
fn http_to_grpc(status: u16) -> u32 {
    match status {
        400 => INTERNAL,
        401 => 16,
        403 => 7,
        404 => 12,
        429 | 502..=504 => 14,
        _ => UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::{grpc_timeout, http_to_grpc, percent_decode, status_of};

    #[test]
    fn timeouts_fit_in_eight_digits() {
        assert_eq!(grpc_timeout(1500), "1500m");
        assert_eq!(grpc_timeout(100_000_000), "100000S");
    }

    #[test]
    fn status_and_message_come_from_the_headers() {
        let h = vec![
            ("grpc-status".to_owned(), "3".to_owned()),
            (
                "grpc-message".to_owned(),
                "bad%20input%3A %C3%A4".to_owned(),
            ),
        ];
        assert_eq!(status_of(&h), Some((3, "bad input: ä".into())));
        assert_eq!(status_of(&[]), None);
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(http_to_grpc(404), 12);
        assert_eq!(http_to_grpc(503), 14);
        assert_eq!(http_to_grpc(500), 2);
    }
}

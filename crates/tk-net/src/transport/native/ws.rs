// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! WebSocket over the native stack. Wave 8, strand A.
//!
//! - `ws://` and `wss://`; TCP and TLS are our own (rustls with the transport's trust, ALPN
//!   `http/1.1` only, since the WebSocket handshake is an HTTP/1.1 upgrade), framing and the
//!   handshake are `tungstenite`'s.
//! - Handshake headers in order; `Sec-WebSocket-Protocol` from `StreamOpen::protocols`.
//!   Headers the handshake sets itself (`Host`, `Connection`, `Upgrade`,
//!   `Sec-WebSocket-Version`, `Sec-WebSocket-Key`) are not taken from the request.
//! - Text and binary frames; pings are answered by `tungstenite`; with `keep_alive_ms` we
//!   send a ping whenever that long passed since the last one.
//! - The connect timeout covers DNS, TCP, TLS and the handshake.
//! - `close()` sends close 1000 and waits up to [`CLOSE_WAIT`] for the peer's close frame;
//!   the session then ends with `closed` (`by_client: true`). A peer that closes sends its
//!   code and reason; a connection that drops without a close frame ends with code 1006.
//! - Client certificates are not presented (the `StreamOpen` carries none).

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use http::Uri;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use web_time::Instant;

use super::NativeTransport;
use crate::LocalBoxFuture;
use crate::stream::{Frame, StreamEvent, StreamHead, StreamOpen, StreamSession};
use crate::transport::{HttpVersion, TransportError};

/// How long `close()` waits for the peer's close frame.
const CLOSE_WAIT: Duration = Duration::from_secs(2);

/// Headers the handshake writes itself.
const HANDSHAKE_HEADERS: [&str; 5] = [
    "host",
    "connection",
    "upgrade",
    "sec-websocket-version",
    "sec-websocket-key",
];

pub(super) async fn open<'a>(
    transport: &'a NativeTransport,
    open: StreamOpen,
) -> Result<Box<dyn StreamSession + 'a>, TransportError> {
    let StreamOpen::WebSocket {
        url,
        headers,
        protocols,
        timeout_ms,
        keep_alive_ms,
    } = open
    else {
        return Err(TransportError::Other("not a WebSocket session".into()));
    };
    let started = Instant::now();
    let connect = connect(transport, &url, &headers, &protocols);
    let (ws, head) = match timeout_ms {
        Some(ms) => tokio::time::timeout(Duration::from_millis(ms), connect)
            .await
            .map_err(|_| TransportError::Timeout)??,
        None => connect.await?,
    };
    let keep_alive = keep_alive_ms.map(Duration::from_millis);
    Ok(Box::new(WsSession {
        ws,
        head: Some(head),
        started,
        keep_alive,
        next_ping: keep_alive.map(|k| tokio::time::Instant::now() + k),
        state: State::Open,
    }))
}

async fn connect(
    transport: &NativeTransport,
    url: &str,
    headers: &[(String, String)],
    protocols: &[String],
) -> Result<(WebSocketStream<WsStream>, StreamHead), TransportError> {
    let invalid = |why: &str| TransportError::InvalidUrl(format!("{url}: {why}"));
    let without_fragment = url.split('#').next().unwrap_or("");
    let uri: Uri = without_fragment
        .parse()
        .map_err(|e: http::uri::InvalidUri| invalid(&e.to_string()))?;
    let tls = match uri.scheme_str() {
        Some("ws") => false,
        Some("wss") => true,
        _ => return Err(invalid("only ws:// and wss:// URLs are supported")),
    };
    let host = uri
        .host()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| invalid("missing host"))?;
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });

    let tcp = tcp_connect(&host, port).await?;
    let _ = tcp.set_nodelay(true);
    let stream = if tls {
        let connector = transport.tls.connector()?;
        let mut config = (**connector.config()).clone();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let server_name = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|e| TransportError::InvalidUrl(format!("{host}: {e}")))?;
        let stream = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| tls_error(&host, e))?;
        WsStream::Tls(Box::new(stream))
    } else {
        WsStream::Plain(tcp)
    };

    let mut request = without_fragment
        .into_client_request()
        .map_err(|e| invalid(&e.to_string()))?;
    let map = request.headers_mut();
    for (name, value) in headers {
        if HANDSHAKE_HEADERS
            .iter()
            .any(|h| h.eq_ignore_ascii_case(name))
        {
            continue;
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| TransportError::Other(format!("invalid header name: {name:?}")))?;
        let header_value = HeaderValue::from_bytes(value.as_bytes()).map_err(|_| {
            TransportError::Other(format!("invalid value for header {name}: {value:?}"))
        })?;
        map.append(header_name, header_value);
    }
    if !protocols.is_empty() {
        let value = HeaderValue::from_str(&protocols.join(", "))
            .map_err(|_| TransportError::Other("invalid Sec-WebSocket-Protocol".into()))?;
        map.insert("sec-websocket-protocol", value);
    }
    let (ws, response) = tokio_tungstenite::client_async_with_config(request, stream, None)
        .await
        .map_err(handshake_error)?;
    let head = StreamHead {
        status: response.status().as_u16(),
        version: HttpVersion::Http11,
        headers: response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_owned(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect(),
        url: url.to_owned(),
    };
    Ok((ws, head))
}

async fn tcp_connect(host: &str, port: u16) -> Result<TcpStream, TransportError> {
    let addrs: Vec<std::net::SocketAddr> = match host.parse::<std::net::IpAddr>() {
        Ok(ip) => vec![std::net::SocketAddr::new(ip, port)],
        Err(_) => tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| TransportError::Dns(format!("{host}: {e}")))?
            .collect(),
    };
    if addrs.is_empty() {
        return Err(TransportError::Dns(format!("{host}: no addresses")));
    }
    let mut last_error = None;
    for addr in &addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(format!("{addr}: {e}")),
        }
    }
    Err(TransportError::Connect(
        last_error.unwrap_or_else(|| format!("{host}:{port}")),
    ))
}

fn tls_error(host: &str, e: io::Error) -> TransportError {
    match e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        Some(tls) => TransportError::Tls(format!("{host}: {tls}")),
        None => TransportError::Tls(format!("{host}: {e}")),
    }
}

fn handshake_error(e: WsError) -> TransportError {
    match e {
        WsError::Http(response) => {
            let status = response.status();
            TransportError::Other(format!(
                "the server refused the WebSocket handshake: HTTP {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            ))
        }
        WsError::Io(e) => TransportError::Connect(e.to_string()),
        e => TransportError::Other(format!("WebSocket handshake failed: {e}")),
    }
}

enum State {
    Open,
    /// We sent a close frame; waiting for the peer's until the deadline.
    Closing(tokio::time::Instant),
    Done,
}

struct WsSession {
    ws: WebSocketStream<WsStream>,
    head: Option<StreamHead>,
    started: Instant,
    keep_alive: Option<Duration>,
    next_ping: Option<tokio::time::Instant>,
    state: State,
}

impl WsSession {
    fn at_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn closed(&mut self, code: Option<u32>, reason: String) -> StreamEvent {
        let by_client = matches!(self.state, State::Closing(_));
        self.state = State::Done;
        StreamEvent::Closed {
            at_ms: self.at_ms(),
            code,
            reason,
            trailers: Vec::new(),
            by_client,
        }
    }

    async fn next_event(&mut self) -> Option<StreamEvent> {
        if let Some(head) = self.head.take() {
            return Some(StreamEvent::Opened {
                at_ms: self.at_ms(),
                head,
            });
        }
        loop {
            let deadline = match self.state {
                State::Done => return None,
                State::Closing(deadline) => Some(deadline),
                State::Open => self.next_ping,
            };
            let item = match deadline {
                Some(deadline) => match tokio::time::timeout_at(deadline, self.ws.next()).await {
                    Ok(item) => item,
                    Err(_) => {
                        if matches!(self.state, State::Closing(_)) {
                            // The peer did not answer our close frame in time.
                            return Some(self.closed(Some(1000), String::new()));
                        }
                        if let Some(interval) = self.keep_alive {
                            self.next_ping = Some(tokio::time::Instant::now() + interval);
                        }
                        if let Err(e) = self.ws.send(Message::Ping(Default::default())).await {
                            return Some(self.failed(e));
                        }
                        continue;
                    }
                },
                None => self.ws.next().await,
            };
            match item {
                None => return Some(self.dropped()),
                Some(Ok(Message::Text(text))) => {
                    return Some(self.message(Frame::Text(text.as_str().to_owned())));
                }
                Some(Ok(Message::Binary(bytes))) => {
                    return Some(self.message(Frame::Binary(bytes.to_vec())));
                }
                Some(Ok(Message::Close(frame))) => {
                    let (code, reason) = match frame {
                        Some(f) => (Some(u32::from(u16::from(f.code))), f.reason.to_string()),
                        // A close frame without a code (RFC 6455: 1005, "no status").
                        None => (Some(1005), String::new()),
                    };
                    return Some(self.closed(code, reason));
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
                Some(Err(WsError::ConnectionClosed | WsError::AlreadyClosed)) => {
                    return Some(self.dropped());
                }
                // After our close frame, a peer that just hangs up has closed too.
                Some(Err(_)) if matches!(self.state, State::Closing(_)) => {
                    return Some(self.dropped());
                }
                Some(Err(e)) => return Some(self.failed(e)),
            }
        }
    }

    fn message(&self, frame: Frame) -> StreamEvent {
        StreamEvent::Message {
            at_ms: self.at_ms(),
            direction: crate::stream::Direction::Received,
            frame,
            name: None,
            id: None,
        }
    }

    /// The connection ended without a close frame (or after ours).
    fn dropped(&mut self) -> StreamEvent {
        if matches!(self.state, State::Closing(_)) {
            self.closed(Some(1000), String::new())
        } else {
            self.closed(
                Some(1006),
                "the connection closed without a close frame".into(),
            )
        }
    }

    fn failed(&mut self, e: WsError) -> StreamEvent {
        self.state = State::Done;
        StreamEvent::Error {
            at_ms: self.at_ms(),
            error: match e {
                WsError::Io(e) => TransportError::Other(format!("WebSocket connection: {e}")),
                e => TransportError::Other(format!("WebSocket: {e}")),
            },
        }
    }
}

impl StreamSession for WsSession {
    fn send(&mut self, frame: Frame) -> LocalBoxFuture<'_, Result<(), TransportError>> {
        Box::pin(async move {
            if !matches!(self.state, State::Open) {
                return Err(TransportError::Other("the WebSocket is closed".into()));
            }
            let message = match frame {
                Frame::Text(text) => Message::text(text),
                Frame::Binary(bytes) => Message::binary(bytes),
            };
            self.ws
                .send(message)
                .await
                .map_err(|e| TransportError::Other(format!("WebSocket send: {e}")))
        })
    }

    fn finish(&mut self) -> LocalBoxFuture<'_, Result<(), TransportError>> {
        Box::pin(async { Ok(()) })
    }

    fn next(&mut self) -> LocalBoxFuture<'_, Option<StreamEvent>> {
        Box::pin(self.next_event())
    }

    fn close(&mut self) -> LocalBoxFuture<'_, ()> {
        Box::pin(async move {
            if matches!(self.state, State::Open) {
                self.state = State::Closing(tokio::time::Instant::now() + CLOSE_WAIT);
                let frame = CloseFrame {
                    code: CloseCode::Normal,
                    reason: "".into(),
                };
                // A failure shows as the end of the connection in `next`.
                let _ = self.ws.close(Some(frame)).await;
            }
        })
    }
}

/// TCP or TLS over TCP, as `tungstenite` needs it (tokio's I/O traits).
enum WsStream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for WsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            WsStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            WsStream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for WsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            WsStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            WsStream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            WsStream::Plain(s) => Pin::new(s).poll_flush(cx),
            WsStream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            WsStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            WsStream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

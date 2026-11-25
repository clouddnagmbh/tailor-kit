// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Our own connector for the hyper client: DNS, TCP and TLS as separate, timed steps, so the
//! response can report `dns`, `connect` and `tls`. ALPN decides between HTTP/1.1 and HTTP/2.

use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use http::Uri;
use hyper::rt::{Read, ReadBufCursor, Write};
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use web_time::Instant;

use super::certinfo::{self, TlsRegistry};
use super::tls::TlsSetup;
use crate::transport::TransportError;

/// How a pooled connection was set up. Attached to every response on that connection.
#[derive(Debug, Clone)]
pub(super) struct ConnInfo {
    pub dns: Option<Duration>,
    pub connect: Duration,
    pub tls: Option<Duration>,
    /// When the connection was ready. A request that started after this instant got a reused
    /// connection and reports no setup phases.
    pub ready_at: Instant,
}

#[derive(Clone)]
pub(super) struct Connector {
    tls: Arc<TlsSetup>,
    /// Where the TLS details of each new connection are kept (wave 19, strand R).
    seen: Arc<TlsRegistry>,
}

impl Connector {
    pub fn new(tls: Arc<TlsSetup>, seen: Arc<TlsRegistry>) -> Self {
        Self { tls, seen }
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl tower_service::Service<Uri> for Connector {
    type Response = Conn;
    type Error = TransportError;
    type Future = BoxFuture<Result<Conn, TransportError>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let tls = self.tls.clone();
        let seen = self.seen.clone();
        Box::pin(connect(tls, seen, uri))
    }
}

async fn connect(
    tls: Arc<TlsSetup>,
    seen: Arc<TlsRegistry>,
    uri: Uri,
) -> Result<Conn, TransportError> {
    let https = match uri.scheme_str() {
        Some("http") => false,
        Some("https") => true,
        _ => return Err(TransportError::InvalidUrl(uri.to_string())),
    };
    let host = uri
        .host()
        .ok_or_else(|| TransportError::InvalidUrl(uri.to_string()))?;
    // `Uri::host` keeps the brackets of IPv6 literals.
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });

    let (addrs, dns) = match host.parse::<IpAddr>() {
        Ok(ip) => (vec![SocketAddr::new(ip, port)], None),
        Err(_) => {
            let started = Instant::now();
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
                .await
                .map_err(|e| TransportError::Dns(format!("{host}: {e}")))?
                .collect();
            if addrs.is_empty() {
                return Err(TransportError::Dns(format!("{host}: no addresses")));
            }
            (addrs, Some(started.elapsed()))
        }
    };

    // Addresses in resolver order; the first that accepts wins (e.g. `localhost` may resolve
    // to ::1 first while the server only listens on 127.0.0.1).
    let started = Instant::now();
    let mut last_error = None;
    let mut tcp = None;
    for addr in &addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                tcp = Some(stream);
                break;
            }
            Err(e) => last_error = Some(format!("{addr}: {e}")),
        }
    }
    let Some(tcp) = tcp else {
        return Err(TransportError::Connect(
            last_error.unwrap_or_else(|| format!("{host}:{port}")),
        ));
    };
    let connect = started.elapsed();
    let _ = tcp.set_nodelay(true);

    if !https {
        return Ok(Conn {
            io: Stream::Plain(TokioIo::new(tcp)),
            h2: false,
            info: ConnInfo {
                dns,
                connect,
                tls: None,
                ready_at: Instant::now(),
            },
        });
    }

    let tls = tls.connector()?;
    let started = Instant::now();
    let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|e| TransportError::InvalidUrl(format!("{host}: {e}")))?;
    let stream = tls
        .connect(server_name, tcp)
        .await
        .map_err(|e| tls_error(host, e))?;
    let tls_time = started.elapsed();
    let session = stream.get_ref().1;
    let h2 = session.alpn_protocol() == Some(b"h2");
    seen.record(
        host,
        port,
        certinfo::info_of(
            session.protocol_version(),
            session.negotiated_cipher_suite(),
            session
                .peer_certificates()
                .and_then(|chain| chain.first())
                .map(|leaf| leaf.as_ref()),
        ),
    );
    Ok(Conn {
        io: Stream::Tls(Box::new(TokioIo::new(stream))),
        h2,
        info: ConnInfo {
            dns,
            connect,
            tls: Some(tls_time),
            ready_at: Instant::now(),
        },
    })
}

fn tls_error(host: &str, e: io::Error) -> TransportError {
    // rustls reports handshake failures (certificate, protocol) as `InvalidData` wrapping a
    // `rustls::Error`; anything else is the peer going away mid-handshake.
    match e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        Some(tls) => TransportError::Tls(format!("{host}: {tls}")),
        None => TransportError::Tls(format!("{host}: {e}")),
    }
}

enum Stream {
    Plain(TokioIo<TcpStream>),
    Tls(Box<TokioIo<TlsStream<TcpStream>>>),
}

/// A connection handed to hyper.
pub(super) struct Conn {
    io: Stream,
    h2: bool,
    info: ConnInfo,
}

impl Connection for Conn {
    fn connected(&self) -> Connected {
        let connected = Connected::new().extra(self.info.clone());
        if self.h2 {
            connected.negotiated_h2()
        } else {
            connected
        }
    }
}

impl Read for Conn {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.get_mut().io {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl Write for Conn {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.get_mut().io {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().io {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().io {
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match &self.io {
            Stream::Plain(s) => s.is_write_vectored(),
            Stream::Tls(s) => s.is_write_vectored(),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match &mut self.get_mut().io {
            Stream::Plain(s) => Pin::new(s).poll_write_vectored(cx, bufs),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write_vectored(cx, bufs),
        }
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Native transport (macOS, Windows, Linux, iOS): own HTTP/1.1 and HTTP/2 stack with TLS.
//!
//! Owner: wave 2, strand A.
//!
//! - HTTP: hyper's client with connection pooling (`hyper-util`); HTTP/2 when the server
//!   picks `h2` via ALPN, HTTP/1.1 otherwise. Plain `http://` is always HTTP/1.1.
//! - TLS: rustls with the `ring` provider; certificates are checked by the operating system
//!   (`rustls-platform-verifier`), so corporate CAs in the OS trust store work.
//! - Client certificates (wave 5, strand D): a request with `SendOptions::client_identity`
//!   goes through a connection pool of its own that presents that identity, so pooled
//!   connections never mix identities. A redirect to another origin drops the identity (it
//!   was chosen for the first host), like the credential headers.
//! - Timing: our own connector measures DNS, TCP connect and TLS handshake per connection.
//!   A request on a reused connection reports `None` for those phases.
//! - Redirects are followed here, not by hyper, so every hop is visible and the browser rules
//!   apply (see `redirect`).
//! - The timeout in `SendOptions` covers the whole exchange, including reading the body.
//!
//! Must run inside a tokio runtime (the CLI and the desktop app use one).
//!
//! On the wire, HTTP/1.1 header names are sent in lowercase (hyper does not preserve case
//! for client requests); HTTP header names are case-insensitive. A `Content-Length` header in
//! the request is ignored: the length always comes from the actual body.

mod body;
mod connector;
mod redirect;
#[cfg(test)]
mod tests;
mod tls;

use std::error::Error as StdError;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use http::header::{HeaderName, HeaderValue, LOCATION};
use http::{Method, Uri};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use web_time::Instant;

use self::body::NativeBody;
use self::connector::{ConnInfo, Connector};
use self::redirect::NextMethod;
use self::tls::TlsSetup;
use super::{
    ClientIdentity, HttpBody, HttpRequest, HttpVersion, Redirect, Response, ResponseHead, Timing,
    Transport, TransportError,
};

/// Idle pooled connections are closed after this long.
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// At most this much of a redirect response's body is read, so the connection can be reused.
const REDIRECT_DRAIN_LIMIT: usize = 64 * 1024;
/// At most this many client-certificate pools are kept; the oldest is dropped first.
const MAX_IDENTITY_POOLS: usize = 8;

type HttpClient = Client<Connector, Full<Bytes>>;

/// The transport used by the CLI and the desktop app. Cheap to clone; reuses connections.
#[derive(Clone)]
pub struct NativeTransport {
    client: HttpClient,
    tls: Arc<TlsSetup>,
    /// One pool per client certificate in use.
    identities: Arc<Mutex<Vec<(ClientIdentity, HttpClient)>>>,
}

impl std::fmt::Debug for NativeTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeTransport").finish_non_exhaustive()
    }
}

impl NativeTransport {
    /// Creates a transport that trusts the operating system's certificate store.
    ///
    /// Cheap: the TLS configuration (and on Linux, reading the system CA bundle) happens on
    /// the first HTTPS connection; problems there surface as [`TransportError::Tls`].
    pub fn new() -> Result<Self, TransportError> {
        Ok(Self::with_tls(TlsSetup::new(Vec::new())))
    }

    /// Like [`NativeTransport::new`], and additionally trusts the given root certificates
    /// (DER-encoded), e.g. a local development CA or a test CA. Invalid certificates are
    /// reported here.
    #[cfg(not(target_os = "android"))]
    pub fn with_extra_root_certificates(
        der_certificates: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Self, TransportError> {
        let roots = der_certificates
            .into_iter()
            .map(rustls::pki_types::CertificateDer::from)
            .collect();
        let tls = TlsSetup::new(roots);
        tls.connector()?;
        Ok(Self::with_tls(tls))
    }

    fn with_tls(tls: TlsSetup) -> Self {
        let tls = Arc::new(tls);
        Self {
            client: build_client(tls.clone()),
            tls,
            identities: Arc::default(),
        }
    }

    /// The pool for requests presenting `identity` (`None`: the shared pool).
    fn client_for(&self, identity: Option<&ClientIdentity>) -> HttpClient {
        let Some(identity) = identity else {
            return self.client.clone();
        };
        let mut pools = self.identities.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, client)) = pools.iter().find(|(id, _)| id == identity) {
            return client.clone();
        }
        let tls = TlsSetup::with_identity(self.tls.extra_roots().to_vec(), Some(identity.clone()));
        let client = build_client(Arc::new(tls));
        if pools.len() >= MAX_IDENTITY_POOLS {
            pools.remove(0);
        }
        pools.push((identity.clone(), client.clone()));
        client
    }

    async fn send_until_head(
        &self,
        request: HttpRequest,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<Response, TransportError> {
        let HttpRequest {
            mut method,
            mut url,
            mut headers,
            mut body,
            options,
        } = request;
        let mut uri = parse_url(&url)?;
        if let Some(HttpBody {
            content_type: Some(content_type),
            ..
        }) = &body
            && !headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        {
            headers.push(("Content-Type".to_owned(), content_type.clone()));
        }
        let mut redirects = Vec::new();
        let mut client = self.client_for(options.client_identity.as_ref());

        loop {
            let req = build_request(&method, &uri, &headers, body.as_ref())?;
            let sent_at = Instant::now();
            let res = client.request(req).await.map_err(|e| client_error(&e))?;
            let head_at = Instant::now();
            let status = res.status().as_u16();

            if options.follow_redirects
                && redirect::is_redirect(status)
                && let Some(location) = res.headers().get(LOCATION)
            {
                if redirects.len() as u32 >= options.max_redirects {
                    return Err(TransportError::TooManyRedirects(options.max_redirects));
                }
                let location = String::from_utf8_lossy(location.as_bytes()).into_owned();
                let next_url = redirect::resolve(&url, &location);
                let next_uri = parse_url(&next_url)?;
                drain(res.into_body()).await;
                if redirect::next_method(status, &method) == NextMethod::GetWithoutBody {
                    method = "GET".to_owned();
                    body = None;
                    headers.retain(|(k, _)| !redirect::is_body_header(k));
                }
                if !redirect::same_origin(&uri, &next_uri) {
                    headers.retain(|(k, _)| !redirect::is_credential_header(k));
                    client = self.client.clone();
                }
                redirects.push(Redirect {
                    status,
                    location: next_url.clone(),
                });
                url = next_url;
                uri = next_uri;
                continue;
            }

            // Setup phases only count if this request waited for them.
            let fresh = res
                .extensions()
                .get::<ConnInfo>()
                .filter(|info| info.ready_at >= sent_at)
                .cloned();
            let request_started = fresh.as_ref().map_or(sent_at, |info| info.ready_at);
            let timing = Timing {
                dns: fresh.as_ref().and_then(|info| info.dns),
                connect: fresh.as_ref().map(|info| info.connect),
                tls: fresh.as_ref().and_then(|info| info.tls),
                ttfb: head_at.saturating_duration_since(request_started),
            };
            let version = match res.version() {
                http::Version::HTTP_10 => HttpVersion::Http10,
                http::Version::HTTP_11 => HttpVersion::Http11,
                http::Version::HTTP_2 => HttpVersion::Http2,
                http::Version::HTTP_3 => HttpVersion::Http3,
                _ => HttpVersion::Unknown,
            };
            let response_headers = res
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.as_str().to_owned(),
                        String::from_utf8_lossy(v.as_bytes()).into_owned(),
                    )
                })
                .collect();
            let head = ResponseHead {
                status,
                version,
                headers: response_headers,
                url,
                redirects,
                timing,
            };
            return Ok(Response {
                head,
                body: Box::pin(NativeBody::new(res.into_body(), deadline)),
            });
        }
    }
}

impl Transport for NativeTransport {
    async fn send(&self, request: HttpRequest) -> Result<Response, TransportError> {
        let deadline = request
            .options
            .timeout
            .and_then(|t| tokio::time::Instant::now().checked_add(t));
        let exchange = self.send_until_head(request, deadline);
        match deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, exchange)
                .await
                .map_err(|_| TransportError::Timeout)?,
            None => exchange.await,
        }
    }
}

fn build_client(tls: Arc<TlsSetup>) -> HttpClient {
    Client::builder(TokioExecutor::new())
        .timer(TokioTimer::new())
        .pool_timer(TokioTimer::new())
        .pool_idle_timeout(POOL_IDLE_TIMEOUT)
        .build(Connector::new(tls))
}

fn parse_url(url: &str) -> Result<Uri, TransportError> {
    let invalid = |why: &str| TransportError::InvalidUrl(format!("{url}: {why}"));
    // The fragment is never sent.
    let without_fragment = url.split('#').next().unwrap_or("");
    let uri: Uri = without_fragment
        .parse()
        .map_err(|e: http::uri::InvalidUri| invalid(&e.to_string()))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return Err(invalid("only http:// and https:// URLs are supported"));
    }
    if uri.host().is_none_or(str::is_empty) {
        return Err(invalid("missing host"));
    }
    Ok(uri)
}

fn build_request(
    method: &str,
    uri: &Uri,
    headers: &[(String, String)],
    body: Option<&HttpBody>,
) -> Result<http::Request<Full<Bytes>>, TransportError> {
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|_| TransportError::Other(format!("invalid method: {method:?}")))?;
    let body = body.map_or_else(Bytes::new, |b| b.bytes.clone());
    let mut req = http::Request::new(Full::new(body));
    *req.method_mut() = method;
    *req.uri_mut() = uri.clone();
    let map = req.headers_mut();
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| TransportError::Other(format!("invalid header name: {name:?}")))?;
        let header_value = HeaderValue::from_bytes(value.as_bytes()).map_err(|_| {
            TransportError::Other(format!("invalid value for header {name}: {value:?}"))
        })?;
        map.append(header_name, header_value);
    }
    Ok(req)
}

/// Reads (a bounded part of) a redirect response's body so its connection can be reused.
async fn drain(body: Incoming) {
    let mut body = std::pin::pin!(NativeBody::new(body, None));
    let mut read = 0;
    while read <= REDIRECT_DRAIN_LIMIT {
        use futures_core::Stream;
        match std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await {
            Some(Ok(chunk)) => read += chunk.len(),
            _ => break,
        }
    }
}

/// `error: cause: cause`, without repeating a cause that the outer message already contains.
fn chain(e: &(dyn StdError + 'static)) -> String {
    let mut message = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        let part = s.to_string();
        if !message.contains(&part) {
            message.push_str(": ");
            message.push_str(&part);
        }
        source = s.source();
    }
    message
}

fn client_error(e: &hyper_util::client::legacy::Error) -> TransportError {
    // Our connector's errors are already `TransportError`s; find them in the chain.
    let mut source: Option<&(dyn StdError + 'static)> = Some(e);
    while let Some(s) = source {
        if let Some(t) = s.downcast_ref::<TransportError>() {
            return t.clone();
        }
        if let Some(h) = s.downcast_ref::<hyper::Error>()
            && h.is_timeout()
        {
            return TransportError::Timeout;
        }
        source = s.source();
    }
    if e.is_connect() {
        TransportError::Connect(chain(e))
    } else {
        TransportError::Other(chain(e))
    }
}

fn hyper_error(e: &hyper::Error) -> TransportError {
    if e.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Other(chain(e))
    }
}

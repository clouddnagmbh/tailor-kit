// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Browser transport: the WHATWG `fetch` API via `web-sys`.
//!
//! Owner: wave 2, strand A.
//!
//! Works in windows and workers (it calls the global `fetch`). The body is read chunk by
//! chunk from the response's `ReadableStream`; dropping the body stream aborts the download.
//!
//! What the browser does not let us do, and how it shows up:
//!
//! - **Forbidden request headers are dropped silently**: `Accept-Charset`, `Accept-Encoding`,
//!   `Access-Control-Request-Headers`, `Access-Control-Request-Method`, `Connection`,
//!   `Content-Length`, `Cookie`, `Cookie2`, `Date`, `DNT`, `Expect`, `Host`, `Keep-Alive`,
//!   `Origin`, `Referer`, `Set-Cookie`, `TE`, `Trailer`, `Transfer-Encoding`, `Upgrade`,
//!   `Via`, every `Proxy-*` and `Sec-*` header, and `X-HTTP-Method`, `X-HTTP-Method-Override`,
//!   `X-Method-Override` when they name a forbidden method. The browser adds its own
//!   `User-Agent`, `Accept-*`, `Origin`, `Referer` and `Sec-Fetch-*` headers.
//! - **Forbidden methods** (`CONNECT`, `TRACE`, `TRACK`) and a body on `GET`/`HEAD` are
//!   rejected; the latter is reported before calling `fetch`.
//! - **CORS, mixed content, network errors**: `fetch` rejects with a `TypeError` that carries
//!   no detail. All of them become [`TransportError::Blocked`]; DNS, connect and TLS errors
//!   cannot be told apart.
//! - **Redirects**: the browser follows them itself (its own limit, about 20) and does not
//!   report the hops, so `redirects` is always empty and `max_redirects` is ignored. With
//!   `follow_redirects = false` the request uses `redirect: "manual"`, and the browser hands
//!   back an *opaque redirect*: status `0`, no headers (no `Location`), no body. That is what
//!   this transport returns.
//! - **Response headers**: names are lowercase, repeated headers are joined with `", "`,
//!   `Set-Cookie` is never visible, and cross-origin responses only show the CORS-safelisted
//!   headers plus those in `Access-Control-Expose-Headers`.
//! - **Body**: `Content-Encoding` (gzip, br, ...) is decoded by the browser, so the body and
//!   `size` are the decoded bytes.
//! - **Client certificates** (mTLS) cannot be presented: a request with
//!   `SendOptions::client_identity` fails with [`TransportError::Tls`].
//! - **Version and timing**: the HTTP version is [`HttpVersion::Unknown`]; only `ttfb` is
//!   measured (`fetch` call until the response headers), `dns`, `connect` and `tls` are `None`.

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures_core::Stream;
use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, DomException, Headers, ReadableStreamDefaultReader, RequestInit,
    RequestRedirect, ResponseType,
};
use web_time::Instant;

use super::{HttpRequest, HttpVersion, Response, ResponseHead, Timing, Transport, TransportError};

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
        let HttpRequest {
            method,
            url,
            mut headers,
            body,
            options,
        } = request;
        // Wave 5: `fetch` cannot present a client certificate; fail instead of sending
        // without the one the request needs.
        if options.client_identity.is_some() {
            return Err(TransportError::Tls(
                "the browser cannot present client certificates (mTLS); send this request \
                 from the desktop app or the CLI"
                    .to_owned(),
            ));
        }

        let parsed = web_sys::Url::new(&url)
            .map_err(|e| TransportError::InvalidUrl(format!("{url}: {}", js_message(&e))))?;
        if !matches!(parsed.protocol().as_str(), "http:" | "https:") {
            return Err(TransportError::InvalidUrl(format!(
                "{url}: only http:// and https:// URLs are supported"
            )));
        }
        if body.is_some()
            && (method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD"))
        {
            return Err(TransportError::Other(format!(
                "the browser cannot send a body with {method}"
            )));
        }

        let init = RequestInit::new();
        init.set_method(&method);
        if let Some(content_type) = body.as_ref().and_then(|b| b.content_type.as_ref())
            && !headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        {
            headers.push(("Content-Type".to_owned(), content_type.clone()));
        }
        let js_headers = Headers::new().map_err(js_other)?;
        for (name, value) in &headers {
            js_headers
                .append(name, value)
                .map_err(|e| TransportError::Other(format!("header {name}: {}", js_message(&e))))?;
        }
        init.set_headers(&js_headers);
        if let Some(body) = &body {
            init.set_body(&Uint8Array::from(&body.bytes[..]));
        }
        if !options.follow_redirects {
            init.set_redirect(RequestRedirect::Manual);
        }

        let controller = AbortController::new().map_err(js_other)?;
        init.set_signal(Some(&controller.signal()));
        let timer = options
            .timeout
            .map(|t| Timer::start(t, controller.clone()))
            .transpose()?;
        let timed_out = || timer.as_ref().is_some_and(Timer::fired);

        let (global, fetch) = global_function("fetch")?;
        let started = Instant::now();
        let promise: Promise = fetch
            .call2(&global, &JsValue::from_str(&url), &init)
            .map_err(|e| fetch_error(&e, timed_out()))?
            .dyn_into()
            .map_err(js_other)?;
        let res = JsFuture::from(promise)
            .await
            .map_err(|e| fetch_error(&e, timed_out()))?;
        let ttfb = started.elapsed();
        let res: web_sys::Response = res.dyn_into().map_err(js_other)?;

        let mut response_headers = Vec::new();
        if let Ok(Some(entries)) = js_sys::try_iter(&res.headers()) {
            for entry in entries {
                let entry: Array = entry.map_err(js_other)?.unchecked_into();
                if let (Some(name), Some(value)) =
                    (entry.get(0).as_string(), entry.get(1).as_string())
                {
                    response_headers.push((name, value));
                }
            }
        }
        let final_url = match res.url() {
            u if u.is_empty() => url,
            u => u,
        };
        let head = ResponseHead {
            status: if res.type_() == ResponseType::Opaqueredirect {
                0
            } else {
                res.status()
            },
            version: HttpVersion::Unknown,
            headers: response_headers,
            url: final_url,
            redirects: Vec::new(),
            timing: Timing {
                dns: None,
                connect: None,
                tls: None,
                ttfb,
            },
        };

        let body = match res.body() {
            Some(stream) => {
                let reader = ReadableStreamDefaultReader::new(&stream).map_err(js_other)?;
                FetchBody {
                    reader: Some(reader),
                    pending: None,
                    controller,
                    timer,
                }
            }
            // No body (e.g. HEAD, 204, opaque redirect); the timer is no longer needed.
            None => FetchBody {
                reader: None,
                pending: None,
                controller,
                timer: None,
            },
        };
        Ok(Response {
            head,
            body: Box::pin(body),
        })
    }
}

/// A `setTimeout` that aborts the request. Cleared on drop.
struct Timer {
    id: JsValue,
    fired: Rc<Cell<bool>>,
    _callback: Closure<dyn FnMut()>,
}

impl Timer {
    fn start(timeout: Duration, controller: AbortController) -> Result<Self, TransportError> {
        let fired = Rc::new(Cell::new(false));
        let flag = fired.clone();
        let callback = Closure::<dyn FnMut()>::new(move || {
            flag.set(true);
            controller.abort();
        });
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let (global, set_timeout) = global_function("setTimeout")?;
        let id = set_timeout
            .call2(&global, callback.as_ref(), &JsValue::from(ms))
            .map_err(js_other)?;
        Ok(Self {
            id,
            fired,
            _callback: callback,
        })
    }

    fn fired(&self) -> bool {
        self.fired.get()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Must run before the callback is dropped, or a late timer would call a dead closure.
        if let Ok((global, clear)) = global_function("clearTimeout") {
            let _ = clear.call1(&global, &self.id);
        }
    }
}

/// The response body, read from the `ReadableStream` one chunk at a time.
struct FetchBody {
    /// `None` once the body is finished (or there never was one).
    reader: Option<ReadableStreamDefaultReader>,
    pending: Option<JsFuture>,
    controller: AbortController,
    /// Kept until the body is finished, so the timeout also covers reading the body.
    timer: Option<Timer>,
}

impl FetchBody {
    fn finish(&mut self) {
        self.reader = None;
        self.pending = None;
        self.timer = None;
    }
}

impl Stream for FetchBody {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            let Some(reader) = &this.reader else {
                return Poll::Ready(None);
            };
            let pending = this
                .pending
                .get_or_insert_with(|| JsFuture::from(reader.read()));
            match Pin::new(pending).poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => {
                    let timed_out = this.timer.as_ref().is_some_and(Timer::fired);
                    this.finish();
                    return Poll::Ready(Some(Err(fetch_error(&e, timed_out))));
                }
                Poll::Ready(Ok(result)) => {
                    this.pending = None;
                    let done = Reflect::get(&result, &JsValue::from_str("done"))
                        .ok()
                        .and_then(|d| d.as_bool())
                        .unwrap_or(true);
                    if done {
                        this.finish();
                        return Poll::Ready(None);
                    }
                    let value = Reflect::get(&result, &JsValue::from_str("value"))
                        .unwrap_or(JsValue::UNDEFINED);
                    let Some(chunk) = value.dyn_ref::<Uint8Array>() else {
                        this.finish();
                        return Poll::Ready(Some(Err(TransportError::Other(
                            "response body chunk is not a Uint8Array".to_owned(),
                        ))));
                    };
                    if chunk.length() == 0 {
                        continue;
                    }
                    return Poll::Ready(Some(Ok(Bytes::from(chunk.to_vec()))));
                }
            }
        }
    }
}

impl Drop for FetchBody {
    fn drop(&mut self) {
        // Dropped before the end: stop the download.
        if self.reader.is_some() {
            self.controller.abort();
        }
    }
}

fn global_function(name: &str) -> Result<(JsValue, Function), TransportError> {
    let global: JsValue = js_sys::global().into();
    let function = Reflect::get(&global, &JsValue::from_str(name))
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
        .ok_or_else(|| TransportError::Other(format!("`{name}` is not available here")))?;
    Ok((global, function))
}

fn fetch_error(e: &JsValue, timed_out: bool) -> TransportError {
    if timed_out {
        return TransportError::Timeout;
    }
    if let Some(dom) = e.dyn_ref::<DomException>() {
        match dom.name().as_str() {
            "TimeoutError" => return TransportError::Timeout,
            "AbortError" => return TransportError::Other("request aborted".to_owned()),
            _ => {}
        }
    }
    if e.is_instance_of::<js_sys::TypeError>() {
        return TransportError::Blocked(js_message(e));
    }
    TransportError::Other(js_message(e))
}

fn js_message(e: &JsValue) -> String {
    if let Some(error) = e.dyn_ref::<js_sys::Error>() {
        return String::from(error.message());
    }
    if let Some(dom) = e.dyn_ref::<DomException>() {
        return dom.message();
    }
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

fn js_other(e: JsValue) -> TransportError {
    TransportError::Other(js_message(&e))
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The `text/event-stream` parser (WHATWG HTML, "Server-sent events", 9.2.6), shared by the
//! native SSE session and the browser's `fetch` SSE session, and [`SseSession`], a session
//! over a response body. Wave 8, strand A.
//!
//! A response that is not a `2xx` `text/event-stream` ends with an `error` event after a
//! message carrying the start of its body (what the server said).
//!
//! Lines end in CRLF, LF or CR; a leading BOM is dropped; `:` lines are comments. Fields:
//! `event` (the event's name), `data` (lines joined with LF), `id` (the last event id, kept
//! for later events as the spec says; an id containing NUL is ignored); `retry` is ignored
//! (sessions do not reconnect). A blank line dispatches the event unless its data is empty.

/// One dispatched event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field; `None` for the default type `message`.
    pub name: Option<String>,
    pub data: String,
    /// The last event id, if one was set.
    pub id: Option<String>,
}

/// Incremental parser: feed bytes as they arrive, take the events.
#[derive(Debug, Default)]
pub struct SseParser {
    /// Bytes of an unfinished line.
    line: Vec<u8>,
    /// The previous chunk ended in CR: a LF at the start of the next one belongs to it.
    after_cr: bool,
    started: bool,
    name: Option<String>,
    data: String,
    has_data: bool,
    last_id: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses `bytes` and returns the events completed by them.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        let mut events = Vec::new();
        let mut bytes = bytes;
        if !self.started && !bytes.is_empty() {
            self.line.extend_from_slice(bytes);
            const BOM: &[u8] = b"\xEF\xBB\xBF";
            if self.line.len() < BOM.len() && BOM.starts_with(&self.line) {
                // Wait for more bytes to decide.
                return events;
            }
            self.started = true;
            let pending = std::mem::take(&mut self.line);
            let pending = pending.strip_prefix(BOM).unwrap_or(&pending).to_vec();
            return self.feed_lines(&pending, events);
        }
        if self.after_cr && bytes.first() == Some(&b'\n') {
            bytes = &bytes[1..];
        }
        self.after_cr = false;
        self.feed_lines(bytes, std::mem::take(&mut events))
    }

    fn feed_lines(&mut self, bytes: &[u8], mut events: Vec<SseEvent>) -> Vec<SseEvent> {
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => {
                    self.end_line(&mut events);
                    i += 1;
                }
                b'\r' => {
                    self.end_line(&mut events);
                    if i + 1 < bytes.len() {
                        if bytes[i + 1] == b'\n' {
                            i += 1;
                        }
                    } else {
                        self.after_cr = true;
                    }
                    i += 1;
                }
                b => {
                    self.line.push(b);
                    i += 1;
                }
            }
        }
        events
    }

    /// The stream ended: an event without its blank line is dropped (as the spec says).
    pub fn finish(&mut self) {
        self.line.clear();
        self.reset_event();
    }

    fn end_line(&mut self, events: &mut Vec<SseEvent>) {
        let line = String::from_utf8_lossy(&std::mem::take(&mut self.line)).into_owned();
        if line.is_empty() {
            if self.has_data {
                let mut data = std::mem::take(&mut self.data);
                if data.ends_with('\n') {
                    data.pop();
                }
                events.push(SseEvent {
                    name: self.name.take().filter(|n| !n.is_empty()),
                    data,
                    id: self.last_id.clone(),
                });
            }
            self.reset_event();
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_str(), ""),
        };
        match field {
            "event" => self.name = Some(value.to_owned()),
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
                self.has_data = true;
            }
            "id" if !value.contains('\0') => {
                self.last_id = (!value.is_empty()).then(|| value.to_owned());
            }
            _ => {}
        }
    }

    fn reset_event(&mut self) {
        self.name = None;
        self.data.clear();
        self.has_data = false;
    }
}

// ---------------------------------------------------------------------------------------------
// A session over a response body

use std::collections::VecDeque;

use web_time::Instant;

use super::{Direction, Frame, StreamEvent, StreamHead, StreamSession};
use crate::services::LocalBoxFuture;
use crate::transport::{BodyStream, TransportError};

/// At most this much of a response that is not an event stream is shown.
const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// An SSE session over any response body (also used by the browser's `fetch` session).
pub struct SseSession {
    head: Option<StreamHead>,
    body: Option<BodyStream>,
    parser: SseParser,
    pending: VecDeque<StreamEvent>,
    started: Instant,
    /// Not an event stream: the reason for the final error.
    refused: Option<String>,
    done: bool,
}

impl SseSession {
    pub fn new(head: StreamHead, body: BodyStream, started: Instant) -> Self {
        let content_type = head
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.to_ascii_lowercase())
            .unwrap_or_default();
        let ok = (200..300).contains(&head.status)
            && content_type.trim_start().starts_with("text/event-stream");
        let refused = (!ok).then(|| {
            format!(
                "the server answered HTTP {} with {}, not an event stream",
                head.status,
                if content_type.is_empty() {
                    "no content type".to_owned()
                } else {
                    content_type
                }
            )
        });
        Self {
            head: Some(head),
            body: Some(body),
            parser: SseParser::new(),
            pending: VecDeque::new(),
            started,
            refused,
            done: false,
        }
    }

    /// Sends `request` over any [`Transport`](crate::transport::Transport) (the browser's
    /// `fetch`) and reads the response as a session. The request goes out without its
    /// timeout, which would also cover the endless body; a caller that wants a limit for the
    /// response head races this future against a timer.
    pub async fn open_over<T: crate::transport::Transport>(
        transport: &T,
        mut request: crate::transport::HttpRequest,
    ) -> Result<Self, TransportError> {
        let started = Instant::now();
        request.options.timeout = None;
        let response = transport.send(request).await?;
        let head = StreamHead {
            status: response.head.status,
            version: response.head.version,
            headers: response.head.headers.clone(),
            url: response.head.url.clone(),
        };
        Ok(Self::new(head, response.body, started))
    }

    fn at_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn end(&mut self, event: StreamEvent) -> Option<StreamEvent> {
        self.done = true;
        self.body = None;
        Some(event)
    }

    async fn next_event(&mut self) -> Option<StreamEvent> {
        if let Some(head) = self.head.take() {
            return Some(StreamEvent::Opened {
                at_ms: self.at_ms(),
                head,
            });
        }
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        if self.done {
            return None;
        }
        if let Some(reason) = self.refused.take() {
            let body = self.read_some(ERROR_BODY_LIMIT).await;
            if !body.is_empty() {
                self.pending.push_back(StreamEvent::Error {
                    at_ms: self.at_ms(),
                    error: TransportError::Other(reason),
                });
                self.done = true;
                self.body = None;
                return Some(StreamEvent::Message {
                    at_ms: self.at_ms(),
                    direction: Direction::Received,
                    frame: Frame::Text(String::from_utf8_lossy(&body).into_owned()),
                    name: None,
                    id: None,
                });
            }
            let at_ms = self.at_ms();
            return self.end(StreamEvent::Error {
                at_ms,
                error: TransportError::Other(reason),
            });
        }
        loop {
            let body = self.body.as_mut()?;
            let chunk = std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await;
            match chunk {
                Some(Ok(bytes)) => {
                    let at_ms = self.at_ms();
                    for event in self.parser.feed(&bytes) {
                        self.pending.push_back(StreamEvent::Message {
                            at_ms,
                            direction: Direction::Received,
                            frame: Frame::Text(event.data),
                            name: event.name,
                            id: event.id,
                        });
                    }
                    if let Some(event) = self.pending.pop_front() {
                        return Some(event);
                    }
                }
                Some(Err(error)) => {
                    let at_ms = self.at_ms();
                    return self.end(StreamEvent::Error { at_ms, error });
                }
                None => {
                    self.parser.finish();
                    let at_ms = self.at_ms();
                    return self.end(StreamEvent::Closed {
                        at_ms,
                        code: None,
                        reason: String::new(),
                        trailers: Vec::new(),
                        by_client: false,
                    });
                }
            }
        }
    }

    /// Up to `limit` bytes of the body (for a response that is not an event stream).
    async fn read_some(&mut self, limit: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let Some(body) = self.body.as_mut() else {
            return out;
        };
        while out.len() < limit {
            match std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await {
                Some(Ok(bytes)) => out.extend_from_slice(&bytes),
                _ => break,
            }
        }
        out.truncate(limit);
        out
    }
}

impl StreamSession for SseSession {
    fn send(&mut self, _frame: Frame) -> LocalBoxFuture<'_, Result<(), TransportError>> {
        Box::pin(async {
            Err(TransportError::Other(
                "an SSE session only receives; it cannot send messages".into(),
            ))
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
            if self.done {
                return;
            }
            self.done = true;
            self.body = None;
            self.head = None;
            self.pending.clear();
            let at_ms = self.at_ms();
            self.pending.push_back(StreamEvent::Closed {
                at_ms,
                code: None,
                reason: String::new(),
                trailers: Vec::new(),
                by_client: true,
            });
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_chunks(chunks: &[&[u8]]) -> Vec<SseEvent> {
        let mut parser = SseParser::new();
        chunks.iter().flat_map(|c| parser.feed(c)).collect()
    }

    fn ev(name: Option<&str>, data: &str, id: Option<&str>) -> SseEvent {
        SseEvent {
            name: name.map(str::to_owned),
            data: data.to_owned(),
            id: id.map(str::to_owned),
        }
    }

    #[test]
    fn fields_multiline_data_ids_and_comments() {
        let text = b": comment\nevent: tick\nid: 1\ndata: a\ndata:b\n\ndata: second\n\n";
        assert_eq!(
            parse_chunks(&[text]),
            vec![
                ev(Some("tick"), "a\nb", Some("1")),
                ev(None, "second", Some("1"))
            ]
        );
    }

    #[test]
    fn line_endings_split_across_chunks() {
        let events = parse_chunks(&[b"data: x\r", b"\n\r", b"\ndata: y\r\r"]);
        assert_eq!(events, vec![ev(None, "x", None), ev(None, "y", None)]);
    }

    #[test]
    fn bom_retry_empty_data_and_unfinished_event() {
        let events = parse_chunks(&[b"\xEF\xBB", b"\xBFretry: 10\ndata\n\nevent: x\n\ndata: cut"]);
        // `data` without a colon is an empty data line: dispatched with empty data. `event`
        // alone has no data and is not dispatched; the last event never ends.
        assert_eq!(events, vec![ev(None, "", None)]);
    }

    #[test]
    fn empty_id_resets_and_nul_ids_are_ignored() {
        let events = parse_chunks(&[b"id: 7\ndata: a\n\nid: x\0y\ndata: b\n\nid\ndata: c\n\n"]);
        assert_eq!(
            events,
            vec![
                ev(None, "a", Some("7")),
                ev(None, "b", Some("7")),
                ev(None, "c", None)
            ]
        );
    }
}

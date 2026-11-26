// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Unit tests for [`collect`]; transport-independent.

use std::pin::Pin;
use std::task::{Context, Poll};

use super::*;

/// A body made of the given chunks, optionally ending in an error.
struct Chunks {
    chunks: std::collections::VecDeque<Result<Bytes, TransportError>>,
}

impl Stream for Chunks {
    type Item = Result<Bytes, TransportError>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.chunks.pop_front())
    }
}

fn response(chunks: Vec<Result<Bytes, TransportError>>) -> Response {
    Response {
        head: ResponseHead {
            status: 200,
            version: HttpVersion::Http11,
            headers: vec![],
            url: "http://example.test/".into(),
            redirects: vec![],
            timing: Timing::default(),
        },
        body: Box::pin(Chunks {
            chunks: chunks.into(),
        }),
    }
}

fn ok(s: &'static str) -> Result<Bytes, TransportError> {
    Ok(Bytes::from_static(s.as_bytes()))
}

#[tokio::test]
async fn collect_keeps_everything_below_the_limit() {
    let started = web_time::Instant::now();
    let c = collect(response(vec![ok("hel"), ok(""), ok("lo")]), 100, started)
        .await
        .unwrap();
    assert_eq!(&c.body[..], b"hello");
    assert_eq!(c.size, 5);
    assert!(!c.truncated);
    assert!(c.total <= started.elapsed());
}

#[tokio::test]
async fn collect_cuts_at_the_limit_and_counts_the_rest() {
    let c = collect(
        response(vec![ok("abc"), ok("defg"), ok("hij")]),
        5,
        web_time::Instant::now(),
    )
    .await
    .unwrap();
    assert_eq!(&c.body[..], b"abcde");
    assert_eq!(c.size, 10);
    assert!(c.truncated);
}

#[tokio::test]
async fn collect_exact_limit_is_not_truncated() {
    let c = collect(response(vec![ok("abc")]), 3, web_time::Instant::now())
        .await
        .unwrap();
    assert_eq!(&c.body[..], b"abc");
    assert!(!c.truncated);
}

#[tokio::test]
async fn collect_zero_limit_keeps_nothing() {
    let c = collect(response(vec![ok("abc")]), 0, web_time::Instant::now())
        .await
        .unwrap();
    assert!(c.body.is_empty());
    assert_eq!(c.size, 3);
    assert!(c.truncated);
}

#[tokio::test]
async fn collect_empty_body() {
    let c = collect(response(vec![]), 10, web_time::Instant::now())
        .await
        .unwrap();
    assert!(c.body.is_empty());
    assert_eq!(c.size, 0);
    assert!(!c.truncated);
}

#[tokio::test]
async fn collect_passes_body_errors_through() {
    let err = collect(
        response(vec![ok("abc"), Err(TransportError::Timeout)]),
        10,
        web_time::Instant::now(),
    )
    .await
    .unwrap_err();
    assert_eq!(err, TransportError::Timeout);
}

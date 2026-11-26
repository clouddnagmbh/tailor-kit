// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The response body as a [`Stream`] of chunks, straight from hyper, with the overall deadline.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use hyper::body::{Body, Incoming};

use crate::transport::TransportError;

pub(super) struct NativeBody {
    inner: Incoming,
    deadline: Option<Pin<Box<tokio::time::Sleep>>>,
    done: bool,
}

impl NativeBody {
    pub fn new(inner: Incoming, deadline: Option<tokio::time::Instant>) -> Self {
        Self {
            inner,
            deadline: deadline.map(|d| Box::pin(tokio::time::sleep_until(d))),
            done: false,
        }
    }
}

impl Stream for NativeBody {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        if let Some(deadline) = &mut this.deadline
            && deadline.as_mut().poll(cx).is_ready()
        {
            this.done = true;
            return Poll::Ready(Some(Err(TransportError::Timeout)));
        }
        loop {
            match Pin::new(&mut this.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(data) if !data.is_empty() => return Poll::Ready(Some(Ok(data))),
                    // Empty data frames and trailers carry no body bytes.
                    _ => continue,
                },
                Poll::Ready(Some(Err(e))) => {
                    this.done = true;
                    return Poll::Ready(Some(Err(super::hyper_error(&e))));
                }
                Poll::Ready(None) => {
                    this.done = true;
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

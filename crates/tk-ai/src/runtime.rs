// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Wave 16, strand A: what every shell does around a provider call, once.
//!
//! 1. [`resolve`] picks the profile for a role or id ([`AiSettings::resolve`]).
//! 2. [`prepare_request`] asks the product's [`AiContextSource`] whether the assistant is
//!    allowed (else [`AiError::OptedOut`], nothing is built) and puts its masked context into
//!    the prompt ([`request_call`]); [`preview`] masks the whole call with every secret in
//!    scope and describes it ([`AiPreview`]): the call the shell then sends is the same bytes.
//! 3. [`execute`] streams the call through the provider; a [`CancelToken`] (kept by id in
//!    [`Calls`]) stops it.
//!
//! The shells add only what differs: where settings and keys live, which transport, and how
//! events reach the page.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};

use tk_net::transport::Transport;

use crate::{
    AiCall, AiContextSource, AiDelta, AiError, AiPreview, AiProfile, AiReply, AiRequest,
    AiSettings, ContextBlock, mask_call, mask_error, provider, request_url,
};

/// Default answer size of a generic call.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// A call ready to be previewed or sent, before masking.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub profile: AiProfile,
    pub call: AiCall,
    /// Every secret value in scope; the whole call is masked with them.
    pub secrets: Vec<String>,
    /// The id of an [`AiRequest`] (for cancelling), empty for a product's own task.
    pub id: String,
}

/// Resolves the profile and builds the call of `request` from the product's `source`:
/// opt-out check first, then the profile, then the context of the request's scope. Nothing is
/// masked or sent yet.
pub fn prepare_request<C: AiContextSource>(
    source: &C,
    settings: Option<&AiSettings>,
    request: &AiRequest<C::Scope>,
) -> Result<Prepared, AiError> {
    if !source.allowed() {
        return Err(AiError::OptedOut);
    }
    let secrets = source.secrets();
    let profile = resolve(settings, request.profile.as_deref())?.clone();
    let blocks = source.context(&request.scope)?;
    let mut call = request_call(request, &blocks);
    call.tools.extend(source.tools(&request.scope));
    Ok(Prepared {
        profile,
        call,
        secrets,
        id: request.id.clone(),
    })
}

/// The profile for `wanted`, or [`AiError::NotConfigured`].
pub fn resolve<'a>(
    settings: Option<&'a AiSettings>,
    wanted: Option<&str>,
) -> Result<&'a AiProfile, AiError> {
    settings
        .and_then(|s| s.resolve(wanted))
        .ok_or(AiError::NotConfigured)
}

/// The call of `request` with the context `blocks` in the system prompt (still unmasked;
/// [`preview`] masks it).
pub fn request_call<S>(request: &AiRequest<S>, blocks: &[ContextBlock]) -> AiCall {
    let mut system = request.system.clone();
    for block in blocks {
        system.push_str("\n\n");
        system.push_str(&block.heading);
        system.push_str(":\n```json\n");
        system.push_str(&block.json);
        system.push_str("\n```");
    }
    AiCall {
        system,
        messages: request.messages.clone(),
        max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        tools: request.tools.clone(),
        json: request.json.clone(),
    }
}

fn count(text: &str) -> u32 {
    text.matches("***").count() as u32
}

/// Masks `call` with `secrets` and describes it; nothing is sent. `profile` is the profile
/// that will serve it.
pub fn preview(
    settings: &AiSettings,
    profile: &AiProfile,
    call: AiCall,
    secrets: &[String],
) -> Result<AiPreview, AiError> {
    let raw = serde_json::to_string(&call).unwrap_or_default();
    let call = mask_call(call, secrets)?;
    let json = serde_json::to_string(&call).unwrap_or_default();
    let endpoint = request_url(&profile.config, true)
        .map(|u| crate::endpoint_of(&u))
        .unwrap_or_default();
    Ok(AiPreview {
        profile: profile.id.clone(),
        provider: profile.config.provider,
        model: profile.config.model.clone(),
        endpoint,
        masked: count(&json).saturating_sub(count(&raw)),
        bytes: json.len() as u64,
        required: settings.must_preview(&profile.id),
        call,
    })
}

/// A call through `prepared.profile`: mask, send, return the answer's text. The caller has
/// shown the preview if the settings ask for it, and parses the text as its task needs
/// (masking that parse error with the same secrets).
pub async fn ask_text<T: Transport>(
    transport: &T,
    settings: &AiSettings,
    prepared: Prepared,
    key: Option<String>,
    browser: bool,
    cancel: &CancelToken,
) -> Result<String, AiError> {
    let Prepared {
        profile,
        call,
        secrets,
        ..
    } = prepared;
    let shown = preview(settings, &profile, call, &secrets).map_err(|e| mask_error(e, &secrets))?;
    let reply = execute(
        transport,
        &profile,
        key,
        browser,
        shown.call,
        &secrets,
        cancel,
        &|_| {},
    )
    .await?;
    crate::reply_text(reply).map_err(|e| mask_error(e, &secrets))
}

/// Sends `call` (already masked, as in the preview) and streams it. Errors are masked.
#[allow(clippy::too_many_arguments)]
pub async fn execute<T: Transport>(
    transport: &T,
    profile: &AiProfile,
    key: Option<String>,
    browser: bool,
    call: AiCall,
    secrets: &[String],
    cancel: &CancelToken,
    on: &dyn Fn(AiDelta),
) -> Result<AiReply, AiError> {
    if cancel.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    let p =
        provider(transport, &profile.config, key, browser).map_err(|e| mask_error(e, secrets))?;
    cancel
        .guard(p.stream(call, on))
        .await?
        .map_err(|e| mask_error(e, secrets))
}

#[derive(Default)]
struct Inner {
    cancelled: bool,
    waker: Option<Waker>,
}

/// Stops a running call; cheap to clone and to call from another thread.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<Mutex<Inner>>);

impl CancelToken {
    pub fn cancel(&self) {
        let mut i = self.0.lock().unwrap_or_else(|p| p.into_inner());
        i.cancelled = true;
        if let Some(w) = i.waker.take() {
            w.wake();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).cancelled
    }

    /// `fut`, or [`AiError::Cancelled`] as soon as [`CancelToken::cancel`] is called (the
    /// future is dropped, which closes the connection).
    pub async fn guard<F: Future>(&self, fut: F) -> Result<F::Output, AiError> {
        let mut fut = std::pin::pin!(fut);
        std::future::poll_fn(|cx| {
            {
                let mut i = self.0.lock().unwrap_or_else(|p| p.into_inner());
                if i.cancelled {
                    return Poll::Ready(Err(AiError::Cancelled));
                }
                i.waker = Some(cx.waker().clone());
            }
            fut.as_mut().poll(cx).map(Ok)
        })
        .await
    }
}

/// Running calls by id, for `cancel(id)`.
#[derive(Clone, Default)]
pub struct Calls(Arc<Mutex<HashMap<String, CancelToken>>>);

/// Removes the call from [`Calls`] when dropped.
pub struct CallGuard {
    calls: Calls,
    id: String,
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        self.calls
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.id);
    }
}

impl Calls {
    /// Registers call `id`; the token stops it, the guard unregisters it.
    pub fn start(&self, id: &str) -> (CancelToken, CallGuard) {
        let token = CancelToken::default();
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id.to_owned(), token.clone());
        (
            token,
            CallGuard {
                calls: self.clone(),
                id: id.to_owned(),
            },
        )
    }

    /// Stops call `id` if it runs.
    pub fn cancel(&self, id: &str) {
        if let Some(t) = self.0.lock().unwrap_or_else(|p| p.into_inner()).get(id) {
            t.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on;

    #[test]
    fn cancelling_stops_a_pending_future() {
        let token = CancelToken::default();
        let t2 = token.clone();
        let r = block_on(token.guard(async {
            t2.cancel();
            std::future::pending::<()>().await;
        }));
        assert_eq!(r.unwrap_err(), AiError::Cancelled);
        assert!(token.is_cancelled());
    }

    #[test]
    fn calls_register_and_unregister() {
        let calls = Calls::default();
        let (token, guard) = calls.start("a");
        calls.cancel("a");
        assert!(token.is_cancelled());
        drop(guard);
        calls.cancel("a");
    }
}

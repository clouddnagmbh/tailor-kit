// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The seam between the kit and a product's data (N5).
//!
//! A product (an API client, a Git client) knows what its users work on; the kit does not. For
//! a call the kit therefore asks the product through [`AiContextSource`] for exactly three
//! things and nothing else:
//!
//! 1. whether the assistant may be used at all ([`AiContextSource::allowed`]; the product's
//!    opt-out),
//! 2. the context for the call's scope as already masked text ([`ContextBlock`]), and
//! 3. every secret value in scope ([`AiContextSource::secrets`]), so the kit masks the whole
//!    outgoing call once more, string by string and serialized, before it leaves.
//!
//! The scope is the product's own type ([`AiContextSource::Scope`], carried by
//! [`crate::AiRequest`]): the kit stores and returns it but never looks inside. Tools the
//! product offers on top of the caller's own come from [`AiContextSource::tools`].

use crate::{AiError, AiTool};

/// One piece of masked context, as it goes into the system prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBlock {
    /// One line that tells the model what the block is and that it is data, never
    /// instructions (the product's wording).
    pub heading: String,
    /// The masked data, typically pretty-printed JSON.
    pub json: String,
}

/// What a product hands the kit to build one call. Nothing here may contain an unmasked
/// secret; the kit masks again as a second line of defence, never as the first.
pub trait AiContextSource {
    /// What of the product's data a caller may ask to include (the product's own type).
    type Scope;

    /// Whether the assistant may be used now (a project or device opt-out). `false` makes
    /// the call fail with [`AiError::OptedOut`] before anything is built.
    fn allowed(&self) -> bool {
        true
    }

    /// The masked context of `scope`, in the order it goes into the prompt. Failures reach
    /// the caller as the [`AiError`] the product chooses (the product's own error values
    /// stay with the product).
    fn context(&self, scope: &Self::Scope) -> Result<Vec<ContextBlock>, AiError>;

    /// Every secret value in scope; the whole call is masked with them.
    fn secrets(&self) -> Vec<String>;

    /// Tools the product adds to those of the request (default: none).
    fn tools(&self, _scope: &Self::Scope) -> Vec<AiTool> {
        Vec::new()
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The names a product gives the plugin ABI. The shape is fixed:
//!
//! - export `memory` (32-bit linear memory);
//! - `alloc(len: i32) -> i32` and `dealloc(ptr: i32, len: i32)`: the host allocates the input
//!   with `alloc`, and frees the input and the reply of `call` with `dealloc`;
//! - `call(ptr: i32, len: i32) -> i64`: the product's JSON request in, the reply packed as
//!   `(ptr << 32) | len` out;
//! - optional import `<host_module>.<host_function>(ptr: i32, len: i32) -> i64`: a host call,
//!   answered with a buffer the host allocated with `alloc` and the plugin frees;
//! - a small WASI subset (`fd_write` to stdout and stderr, clock, random, empty environment
//!   and arguments, `proc_exit`) so ordinary language runtimes link.
//!
//! The manifest is UTF-8 JSON in the custom section `manifest_section`.

/// The names of one product's ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abi {
    /// Custom section that carries the manifest (`rt-plugin`, `gt-plugin`).
    pub manifest_section: &'static str,
    pub alloc: &'static str,
    pub dealloc: &'static str,
    pub call: &'static str,
    /// Module and name of the host-call import.
    pub host_module: &'static str,
    pub host_function: &'static str,
}

impl Abi {
    /// RequestTailor's names, kept here as the reference for the ABI and for the tests.
    pub const REQUEST_TAILOR: Abi = Abi {
        manifest_section: "rt-plugin",
        alloc: "rt_alloc",
        dealloc: "rt_dealloc",
        call: "rt_call",
        host_module: "rt",
        host_function: "host",
    };
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! WAT plugins for the host tests (shared by the test files).
#![allow(dead_code)]

use serde::Deserialize;
use tk_plugin_host::{Abi, PluginManifest};

/// A second product's names, to show the ABI is a parameter.
pub const TEST_ABI: Abi = Abi {
    manifest_section: "test-plugin",
    alloc: "tp_alloc",
    dealloc: "tp_dealloc",
    call: "tp_call",
    host_module: "tp",
    host_function: "host",
};

pub const API: u32 = 2;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: u32,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub network: Vec<String>,
}

impl PluginManifest for Manifest {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn version(&self) -> &str {
        &self.version
    }
    fn api(&self) -> u32 {
        self.api
    }
    fn network(&self) -> &[String] {
        &self.permissions.network
    }
}

pub fn manifest(id: &str, version: &str, network: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "id": id, "name": id, "version": version, "api": 1,
        "permissions": {"network": network}
    })
}

/// What `call` does.
pub enum Body {
    /// Returns the input unchanged.
    Echo,
    /// Sends the input to the host and returns the host's reply.
    Host,
    /// Writes to stdout through the WASI subset, then returns `{"ok":null}`.
    Log,
    /// Runs forever.
    Loop,
    Trap,
    /// Grows memory by this many pages (traps if the host refuses), then returns `{}`.
    Grow(u32),
    /// Counts its calls in a global and returns the count as one byte: a fresh instance
    /// always answers 1.
    Counter,
    /// Traps in `dealloc` from this call on (counting from 1).
    DeallocTrapsFrom(u32),
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            0x20..=0x7e if b != b'"' && b != b'\\' => out.push(b as char),
            _ => out.push_str(&format!("\\{b:02x}")),
        }
    }
    out
}

pub fn plugin(abi: &Abi, manifest: &serde_json::Value, body: Body) -> Vec<u8> {
    plugin_with_pages(abi, manifest, body, 2)
}

pub fn plugin_with_pages(
    abi: &Abi,
    manifest: &serde_json::Value,
    body: Body,
    pages: u32,
) -> Vec<u8> {
    let (alloc, dealloc, call, hm, hf) = (
        abi.alloc,
        abi.dealloc,
        abi.call,
        abi.host_module,
        abi.host_function,
    );
    let (imports, extra, call_body, dealloc_body) = match body {
        Body::Echo => (
            String::new(),
            String::new(),
            "(i64.or (i64.shl (i64.extend_i32_u (local.get 0)) (i64.const 32)) (i64.extend_i32_u (local.get 1)))".to_owned(),
            String::new(),
        ),
        Body::Host => (
            format!(r#"(import "{hm}" "{hf}" (func $host (param i32 i32) (result i64)))"#),
            String::new(),
            "(call $host (local.get 0) (local.get 1))".to_owned(),
            String::new(),
        ),
        Body::Log => (
            r#"(import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
               (import "wasi_snapshot_preview1" "clock_time_get" (func (param i32 i64 i32) (result i32)))
               (import "wasi_snapshot_preview1" "random_get" (func $random (param i32 i32) (result i32)))
               (import "wasi_snapshot_preview1" "environ_sizes_get" (func (param i32 i32) (result i32)))
               (import "wasi_snapshot_preview1" "proc_exit" (func (param i32)))"#
                .to_owned(),
            r#"(data (i32.const 100) "hello from js\n") (data (i32.const 200) "{\"ok\":null}")"#.to_owned(),
            "(i32.store (i32.const 0) (i32.const 100)) (i32.store (i32.const 4) (i32.const 14))
             (drop (call $fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))
             (drop (call $random (i32.const 300) (i32.const 16)))
             (i64.or (i64.shl (i64.const 200) (i64.const 32)) (i64.const 11))"
                .to_owned(),
            String::new(),
        ),
        Body::Loop => (String::new(), String::new(), "(loop $l (br $l)) (i64.const 0)".into(), String::new()),
        Body::Trap => (String::new(), String::new(), "unreachable".into(), String::new()),
        Body::Grow(n) => (
            String::new(),
            r#"(data (i32.const 200) "{}")"#.into(),
            format!("(if (i32.eq (memory.grow (i32.const {n})) (i32.const -1)) (then unreachable)) (i64.or (i64.shl (i64.const 200) (i64.const 32)) (i64.const 2))"),
            String::new(),
        ),
        Body::Counter => (
            String::new(),
            "(global $n (mut i32) (i32.const 0))".into(),
            "(global.set $n (i32.add (global.get $n) (i32.const 1)))
             (i32.store8 (i32.const 300) (global.get $n))
             (i64.or (i64.shl (i64.const 300) (i64.const 32)) (i64.const 1))"
                .into(),
            String::new(),
        ),
        Body::DeallocTrapsFrom(n) => (
            String::new(),
            "(global $d (mut i32) (i32.const 0))".into(),
            "(i64.or (i64.shl (i64.const 300) (i64.const 32)) (i64.const 1))".into(),
            format!(
                "(global.set $d (i32.add (global.get $d) (i32.const 1)))
                 (if (i32.ge_u (global.get $d) (i32.const {n})) (then unreachable))"
            ),
        ),
    };
    let wat = format!(
        r#"(module
  {imports}
  (memory (export "memory") {pages})
  {extra}
  (global $bump (mut i32) (i32.const 4096))
  (func (export "{alloc}") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $len)))
    (local.get $p))
  (func (export "{dealloc}") (param i32 i32) {dealloc_body})
  (func (export "{call}") (param i32 i32) (result i64) {call_body})
  (@custom "{section}" "{manifest}"))"#,
        section = abi.manifest_section,
        manifest = escape(&manifest.to_string()),
    );
    wat::parse_str(&wat).unwrap_or_else(|e| panic!("{e}\n{wat}"))
}

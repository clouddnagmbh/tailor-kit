// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The runtime: ABI names chosen by the product, checks before loading, host calls, WASI,
//! fuel and memory limits, a fresh instance per call.

mod support;

use std::cell::RefCell;

use support::{API, Body, Manifest, TEST_ABI, manifest, plugin, plugin_with_pages};
use tk_plugin_host::runtime::NoHostCalls;
use tk_plugin_host::{Abi, HostCalls, Limits, Plugin, PluginError, Step};

fn compile(abi: Abi, wasm: &[u8]) -> Result<Plugin<Manifest>, PluginError> {
    Plugin::compile(abi, API, wasm, &[], true)
}

#[test]
fn the_abi_names_are_the_products() {
    let m = manifest("echo", "1.0.0", &[]);
    for abi in [TEST_ABI, Abi::REQUEST_TAILOR] {
        let wasm = plugin(&abi, &m, Body::Echo);
        let p = compile(abi, &wasm).unwrap();
        let out = p
            .call(Limits::default(), b"{\"a\":1}", &NoHostCalls)
            .unwrap();
        assert_eq!(out.reply, b"{\"a\":1}");
        // A module built for the other product's names is not a plugin of this one.
        let other = if abi == TEST_ABI {
            Abi::REQUEST_TAILOR
        } else {
            TEST_ABI
        };
        assert!(compile(other, &wasm).is_err());
    }
}

#[test]
fn invalid_plugins_do_not_load() {
    let ok = manifest("x", "1", &[]);
    let esc = |v: &serde_json::Value| v.to_string().replace('"', "\\\"");
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (b"not wasm".to_vec(), "not a WebAssembly module"),
        (wat::parse_str("(module)").unwrap(), "no test-plugin section"),
        (
            plugin(&TEST_ABI, &manifest("Bad_Id", "1", &[]), Body::Trap),
            "invalid id",
        ),
        (
            plugin(
                &TEST_ABI,
                &serde_json::json!({"id": "x", "name": "X", "version": "1", "api": 3}),
                Body::Trap,
            ),
            "API version 3",
        ),
        (
            plugin(&TEST_ABI, &manifest("x", "1", &["http://a.test"]), Body::Trap),
            "invalid network permission",
        ),
        (
            wat::parse_str(format!(
                r#"(module (import "env" "system" (func)) (@custom "test-plugin" "{}"))"#,
                esc(&ok)
            ))
            .unwrap(),
            "env.system",
        ),
        (
            // The other product's host import is not offered.
            wat::parse_str(format!(
                r#"(module (import "rt" "host" (func (param i32 i32) (result i64))) (@custom "test-plugin" "{}"))"#,
                esc(&ok)
            ))
            .unwrap(),
            "rt.host",
        ),
        (
            wat::parse_str(format!(
                r#"(module (memory (export "memory") 1) (@custom "test-plugin" "{}"))"#,
                esc(&ok)
            ))
            .unwrap(),
            "missing export",
        ),
        (
            wat::parse_str(format!(
                r#"(module (memory (export "memory") 1)
                   (func (export "tp_alloc") (param i32))
                   (func (export "tp_dealloc") (param i32 i32))
                   (func (export "tp_call") (param i32 i32) (result i64) (i64.const 0))
                   (@custom "test-plugin" "{}"))"#,
                esc(&ok)
            ))
            .unwrap(),
            "tp_alloc has the wrong type",
        ),
    ];
    for (wasm, expected) in cases {
        let err = compile(TEST_ABI, &wasm).unwrap_err();
        assert!(
            err.to_string().contains(expected),
            "{err} should mention {expected}"
        );
    }
}

struct Upper;
impl HostCalls for Upper {
    fn host_call(&self, call: Vec<u8>) -> Vec<u8> {
        call.to_ascii_uppercase()
    }
}

#[test]
fn host_calls_are_answered_synchronously_or_stepped() {
    let wasm = plugin(&TEST_ABI, &manifest("h", "1", &[]), Body::Host);
    let p = compile(TEST_ABI, &wasm).unwrap();
    let out = p.call(Limits::default(), b"ask", &Upper).unwrap();
    assert_eq!(out.reply, b"ASK");

    // The same call, stepped by the caller (an asynchronous shell).
    let (mut run, step) = p.start(Limits::default(), b"ask").unwrap();
    let Step::HostCall(request) = step else {
        panic!("expected a host call");
    };
    assert_eq!(request, b"ask");
    let Step::Done(out) = run.resume(b"later").map_err(|e| e.to_string()).unwrap() else {
        panic!("expected the end");
    };
    assert_eq!(out.reply, b"later");
    assert!(run.resume(b"x").is_err(), "nothing pending any more");
}

struct Recorder(RefCell<Vec<Vec<u8>>>);
impl HostCalls for Recorder {
    fn host_call(&self, call: Vec<u8>) -> Vec<u8> {
        self.0.borrow_mut().push(call);
        b"{}".to_vec()
    }
}

#[test]
fn a_plugin_without_a_host_import_never_reaches_the_host() {
    let wasm = plugin(&TEST_ABI, &manifest("e", "1", &[]), Body::Echo);
    let rec = Recorder(RefCell::new(Vec::new()));
    compile(TEST_ABI, &wasm)
        .unwrap()
        .call(Limits::default(), b"{}", &rec)
        .unwrap();
    assert!(rec.0.borrow().is_empty());
}

#[test]
fn the_wasi_subset_links_and_stdout_goes_to_the_log() {
    let wasm = plugin(&TEST_ABI, &manifest("js", "1", &[]), Body::Log);
    let out = compile(TEST_ABI, &wasm)
        .unwrap()
        .call(Limits::default(), b"{}", &NoHostCalls)
        .unwrap();
    assert_eq!(out.reply, b"{\"ok\":null}");
    assert_eq!(out.log, b"hello from js\n");
}

#[test]
fn fuel_memory_and_traps_fail_the_call_with_the_plugins_name() {
    let m = || manifest("bad", "1", &[]);
    let cases = [
        (plugin(&TEST_ABI, &m(), Body::Loop), "fuel"),
        (plugin(&TEST_ABI, &m(), Body::Trap), "trapped"),
        (plugin(&TEST_ABI, &m(), Body::Grow(1100)), "trapped"),
        (
            plugin_with_pages(&TEST_ABI, &m(), Body::Trap, 1100),
            "memory",
        ),
        (
            plugin(&TEST_ABI, &m(), Body::DeallocTrapsFrom(1)),
            "dealloc failed",
        ),
    ];
    for (wasm, expected) in cases {
        let p = compile(TEST_ABI, &wasm).unwrap();
        let limits = Limits {
            fuel: 5_000_000,
            ..Limits::default()
        };
        let err = p.call(limits, b"{}", &NoHostCalls).unwrap_err();
        assert!(
            matches!(&err, PluginError::Failed { id, message } if id == "bad" && message.contains(expected)),
            "{err:?} should mention {expected}"
        );
    }
    // 64 MiB is the default memory limit: 1024 pages fit, 1025 do not.
    let fits = plugin_with_pages(&TEST_ABI, &m(), Body::Grow(0), 1024);
    compile(TEST_ABI, &fits)
        .unwrap()
        .call(Limits::default(), b"{}", &NoHostCalls)
        .unwrap();
    let small = Limits {
        memory: 1 << 20,
        ..Limits::default()
    };
    assert!(
        compile(TEST_ABI, &fits)
            .unwrap()
            .call(small, b"{}", &NoHostCalls)
            .is_err()
    );
}

#[test]
fn every_call_runs_in_a_fresh_instance() {
    let wasm = plugin(&TEST_ABI, &manifest("c", "1", &[]), Body::Counter);
    let p = compile(TEST_ABI, &wasm).unwrap();
    for _ in 0..3 {
        let out = p.call(Limits::default(), b"{}", &NoHostCalls).unwrap();
        assert_eq!(
            out.reply,
            [1],
            "globals do not survive from one call to the next"
        );
    }
}

#[test]
fn the_host_frees_input_and_reply_and_nothing_else() {
    let m = manifest("f", "1", &[]);
    // Two frees are the contract: input and reply. A plugin whose dealloc breaks on the
    // third would never notice; one that breaks on the second is reported.
    let ok = plugin(&TEST_ABI, &m, Body::DeallocTrapsFrom(3));
    compile(TEST_ABI, &ok)
        .unwrap()
        .call(Limits::default(), b"{}", &NoHostCalls)
        .unwrap();
    let bad = plugin(&TEST_ABI, &m, Body::DeallocTrapsFrom(2));
    let err = compile(TEST_ABI, &bad)
        .unwrap()
        .call(Limits::default(), b"{}", &NoHostCalls)
        .unwrap_err();
    assert!(err.to_string().contains("dealloc failed"), "{err}");
}

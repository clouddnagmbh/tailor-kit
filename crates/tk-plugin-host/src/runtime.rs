// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

//! The WebAssembly side: one shared `wasmi` engine (an interpreter, so it runs natively, on
//! iOS and inside the browser's WebAssembly), the import and export checks of the [`Abi`],
//! and one call in a fresh instance with fuel and memory limits.
//!
//! Host calls suspend the module: the import stores the request and returns a resumable host
//! error; the caller answers it and resumes the module with the reply ([`Run::resume`]). That
//! is how a host call looks synchronous to the plugin on every target, without threads, and
//! how an asynchronous caller (an HTTP request, browser storage) steps a call itself.
//! [`call`] is the synchronous loop over it.

use std::sync::OnceLock;

use wasmi::errors::HostError;
use wasmi::{
    Caller, Config, Engine, Error, Extern, ExternType, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc, TypedResumableCall, TypedResumableCallHostTrap, Val, ValType,
};

use crate::Abi;

/// Limits of one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Fuel (roughly: executed instructions) per call. Fuel is also the time limit: it is
    /// deterministic, so a plugin that is too slow fails the same way on every machine.
    pub fuel: u64,
    /// Linear memory per call, in bytes.
    pub memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: 2_000_000_000,
            memory: 64 * 1024 * 1024,
        }
    }
}

/// Largest message the host reads from a plugin (a reply or a host call).
const MAX_MESSAGE: usize = 32 * 1024 * 1024;
/// Most log output (WASI `fd_write`) kept per call.
const MAX_LOG: usize = 64 * 1024;

/// The engine every plugin runs on. Modules keep only a weak reference to their engine, so
/// one engine for the process keeps them usable wherever they move.
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::default();
        config.consume_fuel(true);
        Engine::new(&config)
    })
}

/// Compiles (and validates) a module and checks its imports and exports against `abi`.
pub fn compile(abi: &Abi, wasm: &[u8]) -> Result<Module, String> {
    let module = Module::new(engine(), wasm).map_err(|e| format!("invalid module: {e}"))?;
    for import in module.imports() {
        let (module_name, name) = (import.module(), import.name());
        let Some((params, results)) = allowed_import(abi, module_name, name) else {
            return Err(format!(
                "imports {module_name}.{name}, which the host does not offer"
            ));
        };
        let ok = matches!(import.ty(), ExternType::Func(f)
            if f.params() == params && f.results() == results);
        if !ok {
            return Err(format!("imports {module_name}.{name} with the wrong type"));
        }
    }
    let mut missing: Vec<&str> = vec!["memory", abi.alloc, abi.dealloc, abi.call];
    for export in module.exports() {
        let name = export.name();
        let want: Option<(&[ValType], &[ValType])> = if name == abi.alloc {
            Some((&[I32], &[I32]))
        } else if name == abi.dealloc {
            Some((&[I32, I32], &[]))
        } else if name == abi.call {
            Some((&[I32, I32], &[I64]))
        } else if name == "memory" {
            if !matches!(export.ty(), ExternType::Memory(m) if !m.is_64()) {
                return Err("export memory is not a 32-bit memory".into());
            }
            None
        } else {
            continue;
        };
        if let Some((params, results)) = want {
            let ok = matches!(export.ty(), ExternType::Func(f)
                if f.params() == params && f.results() == results);
            if !ok {
                return Err(format!("export {name} has the wrong type"));
            }
        }
        missing.retain(|m| *m != name);
    }
    if !missing.is_empty() {
        return Err(format!("missing export(s) {}", missing.join(", ")));
    }
    Ok(module)
}

const I32: ValType = ValType::I32;
const I64: ValType = ValType::I64;

/// Parameter and result types of the imports a plugin may have.
fn allowed_import(
    abi: &Abi,
    module: &str,
    name: &str,
) -> Option<(&'static [ValType], &'static [ValType])> {
    if module == abi.host_module && name == abi.host_function {
        return Some((&[I32, I32], &[I64]));
    }
    if module != WASI {
        return None;
    }
    Some(match name {
        "fd_write" => (&[I32, I32, I32, I32], &[I32]),
        "clock_time_get" => (&[I32, I64, I32], &[I32]),
        "random_get" => (&[I32, I32], &[I32]),
        "environ_get" | "environ_sizes_get" | "args_get" | "args_sizes_get" => {
            (&[I32, I32], &[I32])
        }
        "proc_exit" => (&[I32], &[]),
        _ => return None,
    })
}

const WASI: &str = "wasi_snapshot_preview1";

/// Answers the host calls of one call, synchronously.
pub trait HostCalls {
    /// `call` is the raw message the plugin passed to its host import; the result is the
    /// reply message.
    fn host_call(&self, call: Vec<u8>) -> Vec<u8>;
}

/// A [`HostCalls`] that answers nothing: every host call gets an empty reply.
pub struct NoHostCalls;

impl HostCalls for NoHostCalls {
    fn host_call(&self, _call: Vec<u8>) -> Vec<u8> {
        Vec::new()
    }
}

/// What one call returns: the reply and what the plugin wrote to stdout and stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub reply: Vec<u8>,
    pub log: Vec<u8>,
}

/// Runs the call export once with `input` in a fresh instance of `module`.
pub fn call(
    abi: &Abi,
    module: &Module,
    limits: Limits,
    input: &[u8],
    host: &dyn HostCalls,
) -> Result<Outcome, String> {
    let (mut run, mut step) = Run::start(abi, module, limits, input)?;
    loop {
        match step {
            Step::Done(outcome) => return Ok(outcome),
            Step::HostCall(request) => step = run.resume(&host.host_call(request))?,
        }
    }
}

/// Where a started call stands.
#[derive(Debug)]
pub enum Step {
    /// The plugin finished.
    Done(Outcome),
    /// The plugin waits for the answer to this host call: pass it to [`Run::resume`].
    HostCall(Vec<u8>),
}

struct Data {
    limits: StoreLimits,
    /// The raw host call waiting for its answer while the module is suspended.
    pending: Option<Vec<u8>>,
    log: Vec<u8>,
    rng: u64,
}

/// The marker error that suspends the module at a host call.
#[derive(Debug)]
struct Suspend;

impl std::fmt::Display for Suspend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("host call")
    }
}

impl HostError for Suspend {}

/// One call in progress, in its own instance (memory, globals and fuel are the call's own).
pub struct Run {
    store: Store<Data>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    dealloc: TypedFunc<(i32, i32), ()>,
    input: (i32, i32),
    suspended: Option<TypedResumableCallHostTrap<i64>>,
}

impl std::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Run")
    }
}

fn memory_of(caller: &Caller<'_, Data>) -> Result<Memory, Error> {
    caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .ok_or_else(|| Error::new("the plugin has no memory"))
}

fn slice(memory: &[u8], ptr: i32, len: i32) -> Result<std::ops::Range<usize>, Error> {
    let (ptr, len) = (ptr as u32 as usize, len as u32 as usize);
    let end = ptr
        .checked_add(len)
        .filter(|&e| e <= memory.len())
        .ok_or_else(|| Error::new("out of bounds"))?;
    Ok(ptr..end)
}

fn linker(abi: &Abi) -> Result<Linker<Data>, String> {
    let mut linker = Linker::<Data>::new(engine());
    let e = |e: wasmi::errors::LinkerError| e.to_string();
    linker
        .func_wrap(
            abi.host_module,
            abi.host_function,
            |caller: Caller<'_, Data>, ptr: i32, len: i32| -> Result<i64, Error> {
                let memory = memory_of(&caller)?;
                let data = memory.data(&caller);
                let range = slice(data, ptr, len)?;
                if range.len() > MAX_MESSAGE {
                    return Err(Error::new("host call too large"));
                }
                let bytes = data[range].to_vec();
                let mut caller = caller;
                caller.data_mut().pending = Some(bytes);
                Err(Error::host(Suspend))
            },
        )
        .map_err(e)?;
    linker
        .func_wrap(
            WASI,
            "fd_write",
            |mut caller: Caller<'_, Data>,
             fd: i32,
             iovs: i32,
             count: i32,
             written: i32|
             -> Result<i32, Error> {
                const EBADF: i32 = 8;
                if fd != 1 && fd != 2 {
                    return Ok(EBADF);
                }
                let memory = memory_of(&caller)?;
                let (mem, data) = memory.data_and_store_mut(&mut caller);
                let table = slice(mem, iovs, count.saturating_mul(8))?;
                let mut total: u32 = 0;
                for iov in mem[table].chunks_exact(8) {
                    let ptr = i32::from_le_bytes(iov[..4].try_into().expect("4 bytes"));
                    let len = i32::from_le_bytes(iov[4..].try_into().expect("4 bytes"));
                    let range = slice(mem, ptr, len)?;
                    total = total.saturating_add(range.len() as u32);
                    let room = MAX_LOG.saturating_sub(data.log.len());
                    let bytes = &mem[range];
                    data.log.extend_from_slice(&bytes[..bytes.len().min(room)]);
                }
                let out = slice(mem, written, 4)?;
                mem[out].copy_from_slice(&total.to_le_bytes());
                Ok(0)
            },
        )
        .map_err(e)?;
    linker
        .func_wrap(
            WASI,
            "clock_time_get",
            |mut caller: Caller<'_, Data>, _id: i32, _precision: i64, out: i32| {
                let nanos = web_time::SystemTime::now()
                    .duration_since(web_time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                let memory = memory_of(&caller)?;
                let mem = memory.data_mut(&mut caller);
                let range = slice(mem, out, 8)?;
                mem[range].copy_from_slice(&nanos.to_le_bytes());
                Ok::<i32, Error>(0)
            },
        )
        .map_err(e)?;
    linker
        .func_wrap(
            WASI,
            "random_get",
            |mut caller: Caller<'_, Data>, buf: i32, len: i32| {
                let memory = memory_of(&caller)?;
                let (mem, data) = memory.data_and_store_mut(&mut caller);
                let range = slice(mem, buf, len)?;
                for b in &mut mem[range] {
                    // xorshift64*: hash seeds for language runtimes, not cryptography.
                    data.rng ^= data.rng >> 12;
                    data.rng ^= data.rng << 25;
                    data.rng ^= data.rng >> 27;
                    *b = (data.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 56) as u8;
                }
                Ok::<i32, Error>(0)
            },
        )
        .map_err(e)?;
    for name in ["environ_get", "args_get"] {
        linker
            .func_wrap(WASI, name, |_: Caller<'_, Data>, _: i32, _: i32| 0i32)
            .map_err(e)?;
    }
    for name in ["environ_sizes_get", "args_sizes_get"] {
        linker
            .func_wrap(
                WASI,
                name,
                |mut caller: Caller<'_, Data>, count: i32, size: i32| {
                    let memory = memory_of(&caller)?;
                    let mem = memory.data_mut(&mut caller);
                    for ptr in [count, size] {
                        let range = slice(mem, ptr, 4)?;
                        mem[range].copy_from_slice(&0u32.to_le_bytes());
                    }
                    Ok::<i32, Error>(0)
                },
            )
            .map_err(e)?;
    }
    linker
        .func_wrap(
            WASI,
            "proc_exit",
            |_: Caller<'_, Data>, code: i32| -> Result<(), Error> { Err(Error::i32_exit(code)) },
        )
        .map_err(e)?;
    Ok(linker)
}

/// A message for a failed call.
fn describe(e: &Error) -> String {
    if let Some(code) = e.i32_exit_status() {
        return format!("exited with code {code}");
    }
    if let Some(trap) = e.as_trap_code() {
        if trap == wasmi::TrapCode::OutOfFuel {
            return "ran out of fuel (too much work for one call)".into();
        }
        return format!("trapped: {trap}");
    }
    e.to_string()
}

impl Run {
    /// Instantiates `module` and runs the call export until it finishes or makes its first
    /// host call.
    pub fn start(
        abi: &Abi,
        module: &Module,
        limits: Limits,
        input: &[u8],
    ) -> Result<(Self, Step), String> {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0x9e37_79b9, |d| d.as_nanos() as u64)
            | 1;
        let data = Data {
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory)
                .memories(1)
                .tables(4)
                .table_elements(100_000)
                .instances(1)
                .build(),
            pending: None,
            log: Vec::new(),
            rng: seed,
        };
        let mut store = Store::new(engine(), data);
        store.limiter(|d| &mut d.limits);
        store.set_fuel(limits.fuel).map_err(|e| e.to_string())?;
        let instance = linker(abi)?
            .instantiate_and_start(&mut store, module)
            .map_err(|e| match describe(&e) {
                m if m.contains("memory") || m.contains("limit") => {
                    format!("needs more than {} MiB of memory: {m}", limits.memory >> 20)
                }
                m => m,
            })?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or("missing export memory")?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, abi.alloc)
            .map_err(|e| e.to_string())?;
        let dealloc = instance
            .get_typed_func::<(i32, i32), ()>(&store, abi.dealloc)
            .map_err(|e| e.to_string())?;
        let call = instance
            .get_typed_func::<(i32, i32), i64>(&store, abi.call)
            .map_err(|e| e.to_string())?;
        let mut run = Self {
            store,
            memory,
            alloc,
            dealloc,
            input: (0, 0),
            suspended: None,
        };
        run.input = run.write(input)?;
        let state = call
            .call_resumable(&mut run.store, run.input)
            .map_err(|e| describe(&e))?;
        let step = run.advance(state)?;
        Ok((run, step))
    }

    /// Answers the host call the last [`Step::HostCall`] reported. The reply is copied into
    /// memory the plugin's `alloc` hands out; freeing it is the plugin's job.
    pub fn resume(&mut self, reply: &[u8]) -> Result<Step, String> {
        let suspended = self
            .suspended
            .take()
            .ok_or("resume without a pending host call")?;
        let (ptr, len) = self.write(reply)?;
        let packed = (i64::from(ptr as u32) << 32) | i64::from(len as u32);
        let state = suspended
            .resume(&mut self.store, &[Val::I64(packed)])
            .map_err(|e| describe(&e))?;
        self.advance(state)
    }

    fn advance(&mut self, state: TypedResumableCall<i64>) -> Result<Step, String> {
        match state {
            TypedResumableCall::Finished(packed) => {
                let reply = self.read_packed(packed)?;
                self.free(self.input.0, self.input.1)?;
                self.free((packed >> 32) as i32, packed as i32)?;
                let log = std::mem::take(&mut self.store.data_mut().log);
                Ok(Step::Done(Outcome { reply, log }))
            }
            TypedResumableCall::OutOfFuel(_) => {
                Err("ran out of fuel (too much work for one call)".into())
            }
            TypedResumableCall::HostTrap(suspended) => {
                if suspended.host_error().downcast_ref::<Suspend>().is_none() {
                    return Err(describe(suspended.host_error()));
                }
                let call = self
                    .store
                    .data_mut()
                    .pending
                    .take()
                    .ok_or("host call without a request")?;
                self.suspended = Some(suspended);
                Ok(Step::HostCall(call))
            }
        }
    }

    /// Copies `bytes` into memory allocated with the plugin's `alloc`.
    fn write(&mut self, bytes: &[u8]) -> Result<(i32, i32), String> {
        let len = i32::try_from(bytes.len()).map_err(|_| "message too large")?;
        let ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|e| format!("alloc failed: {}", describe(&e)))?;
        let mem = self.memory.data_mut(&mut self.store);
        let range = slice(mem, ptr, len).map_err(|_| "alloc returned memory out of bounds")?;
        mem[range].copy_from_slice(bytes);
        Ok((ptr, len))
    }

    fn read_packed(&self, packed: i64) -> Result<Vec<u8>, String> {
        let (ptr, len) = ((packed >> 32) as i32, packed as i32);
        let mem = self.memory.data(&self.store);
        let range = slice(mem, ptr, len).map_err(|_| "the reply is out of bounds")?;
        if range.len() > MAX_MESSAGE {
            return Err("the reply is too large".into());
        }
        Ok(mem[range].to_vec())
    }

    fn free(&mut self, ptr: i32, len: i32) -> Result<(), String> {
        self.dealloc
            .call(&mut self.store, (ptr, len))
            .map_err(|e| format!("dealloc failed: {}", describe(&e)))
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! `tk-plugin-host`: the product-neutral part of a WebAssembly plugin host.
//!
//! Plugins are WebAssembly only, on every target. The host interprets them with `wasmi` (no
//! JIT, so the same engine runs natively, on iOS and inside the browser's WebAssembly), meters
//! fuel and memory, runs every call in a fresh instance, checks the manifest and the module
//! before anything of the plugin runs, and checks network permissions.
//!
//! What a product chooses (nothing else is product knowledge here):
//!
//! - the [`Abi`]: name of the manifest section, of the three exports and of the one host
//!   import (RequestTailor: section `rt-plugin`, `rt_alloc`/`rt_dealloc`/`rt_call`, import
//!   `rt.host`; GitTailor: `gt-plugin`, `gt_*`, `gt.host`);
//! - the manifest type ([`PluginManifest`]) and the JSON messages of its API;
//! - the host calls: a plugin writes a request to `<module>.<function>`, the host answers
//!   through [`HostCalls`] (synchronous) or by stepping [`Run`] (asynchronous).

pub mod abi;
pub mod dir;
pub mod manifest;
pub mod permissions;
pub mod runtime;

pub use abi::Abi;
pub use dir::{InstalledRecord, PluginDir};
pub use manifest::PluginManifest;
pub use runtime::{HostCalls, Limits, Outcome, Run, Step};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    /// Not a plugin: not WebAssembly, no or invalid manifest section, bad id, bad module.
    #[error("not a plugin: {0}")]
    Invalid(String),
    /// Built for an API version this host does not run.
    #[error("plugin needs API version {needs}; this host runs 1 to {runs}")]
    ApiVersion { needs: u32, runs: u32 },
    #[error("plugin {0} is not installed")]
    NotFound(String),
    /// The plugin trapped, ran out of fuel or memory, or answered garbage.
    #[error("plugin {id} failed: {message}")]
    Failed { id: String, message: String },
    /// The plugin answered with an error of its own.
    #[error("{0}")]
    Refused(String),
    /// The plugin folder could not be read or written.
    #[error("plugin folder: {0}")]
    Io(String),
}

/// A compiled plugin: cheap to clone, `Send + Sync`, so shells keep them across calls and
/// threads. Every [`Plugin::call`] runs in a fresh instance: memory and globals do not
/// survive from one call to the next.
#[derive(Debug, Clone)]
pub struct Plugin<M: PluginManifest> {
    pub manifest: M,
    /// The network hosts the user granted: the manifest's `network` if the grant covers it,
    /// else nothing.
    pub granted_network: Vec<String>,
    pub enabled: bool,
    /// Shipped with the product (cannot be removed, only switched off).
    pub bundled: bool,
    abi: Abi,
    module: wasmi::Module,
}

impl<M: PluginManifest> Plugin<M> {
    /// Checks the manifest and the module (validation, imports, exports) and compiles it.
    pub fn compile(
        abi: Abi,
        supported_api: u32,
        wasm: &[u8],
        granted_network: &[String],
        enabled: bool,
    ) -> Result<Self, PluginError> {
        let manifest = manifest::read::<M>(&abi, supported_api, wasm)?;
        let module = runtime::compile(&abi, wasm).map_err(PluginError::Invalid)?;
        let granted_network = permissions::effective_network(granted_network, manifest.network());
        Ok(Self {
            manifest,
            granted_network,
            enabled,
            bundled: false,
            abi,
            module,
        })
    }

    pub fn id(&self) -> &str {
        self.manifest.id()
    }

    /// Whether the user granted everything the manifest requests.
    pub fn fully_granted(&self) -> bool {
        permissions::network_covers(&self.granted_network, self.manifest.network())
    }

    fn failed(&self, message: String) -> PluginError {
        PluginError::Failed {
            id: self.id().to_owned(),
            message,
        }
    }

    /// One call in a fresh instance. `input` is the product's JSON request.
    pub fn call(
        &self,
        limits: Limits,
        input: &[u8],
        host: &dyn HostCalls,
    ) -> Result<Outcome, PluginError> {
        runtime::call(&self.abi, &self.module, limits, input, host).map_err(|m| self.failed(m))
    }

    /// Starts a call that the caller steps itself (asynchronous host calls).
    pub fn start(&self, limits: Limits, input: &[u8]) -> Result<(Run, Step), PluginError> {
        Run::start(&self.abi, &self.module, limits, input).map_err(|m| self.failed(m))
    }
}

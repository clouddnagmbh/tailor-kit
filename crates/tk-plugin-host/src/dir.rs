// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

//! The plugin folder: bundled plugins (shipped with the product, in memory), installed plugins
//! (`<id>.wasm` files) and the state file that records which are on.
//!
//! The product chooses the folder and the state file (RequestTailor keeps both in its data
//! folder, GitTailor the folder in `<config>/plugins/` and the state in `<config>/plugins.json`).
//! Inside the folder:
//!
//! - `<id>.wasm`: an installed module, unchanged;
//! - the state file: installation order, the granted network hosts and whether a plugin is on,
//!   for installed and bundled plugins alike (a bundled plugin is on until switched off);
//! - small files of the product's own through [`PluginDir::read_aux`] and
//!   [`PluginDir::write_aux`] (a catalogue, plugin storage).
//!
//! Writes go to a temporary file first and are renamed into place, so a crash never leaves a
//! half-written module or state file. A state file that is damaged or newer than this host
//! understands is never overwritten: it is reported and nothing is written until the user
//! fixes it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Abi, Plugin, PluginError, PluginManifest};

/// The version of the state file this host writes and reads.
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    version: u32,
    #[serde(default)]
    plugins: Vec<Entry>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            plugins: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    id: String,
    #[serde(default)]
    version: String,
    /// Network hosts the user granted (empty for bundled plugins).
    #[serde(default)]
    granted: Vec<String>,
    enabled: bool,
    /// Bundled plugins record only whether they are on.
    #[serde(default)]
    bundled: bool,
}

/// One entry of the state file, as recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRecord {
    pub id: String,
    pub version: String,
    pub granted: Vec<String>,
    pub enabled: bool,
    pub bundled: bool,
}

/// The plugin folder.
#[derive(Debug, Clone)]
pub struct PluginDir {
    dir: PathBuf,
    state_path: PathBuf,
    abi: Abi,
    api: u32,
    bundled: Vec<Vec<u8>>,
}

fn io(e: impl std::fmt::Display) -> PluginError {
    PluginError::Io(e.to_string())
}

/// Writes `bytes` to `path` through a temporary file in the same folder.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// A plain file name (no folders), so a caller cannot reach outside the plugin folder.
fn aux_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\', '\0'])
}

impl PluginDir {
    /// The plugin folder `dir`, with the state in `dir/plugins.json`. `api` is the newest
    /// plugin API version the product runs.
    pub fn new(dir: impl Into<PathBuf>, abi: Abi, api: u32) -> Self {
        let dir = dir.into();
        Self {
            state_path: dir.join("plugins.json"),
            dir,
            abi,
            api,
            bundled: Vec::new(),
        }
    }

    /// Keeps the state file somewhere else.
    pub fn with_state_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.state_path = path.into();
        self
    }

    /// The modules that ship with the product. They load first, cannot be removed and are on
    /// until the user switches them off.
    pub fn with_bundled(mut self, modules: impl IntoIterator<Item = Vec<u8>>) -> Self {
        self.bundled = modules.into_iter().collect();
        self
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    fn module_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.wasm"))
    }

    /// The state, `Default` if there is none, an error if it is damaged or newer.
    fn state(&self) -> Result<State, PluginError> {
        let bytes = match std::fs::read(&self.state_path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(io(e)),
        };
        let state: State = serde_json::from_slice(&bytes).map_err(|e| {
            io(format!(
                "{} is damaged ({e}); it is left as it is",
                self.state_path.display()
            ))
        })?;
        if state.version > STATE_VERSION {
            return Err(io(format!(
                "{} was written by a newer version; it is left as it is",
                self.state_path.display()
            )));
        }
        Ok(state)
    }

    fn save_state(&self, state: &State) -> Result<(), PluginError> {
        let json = serde_json::to_vec_pretty(state).map_err(io)?;
        write_atomic(&self.state_path, &json).map_err(io)
    }

    /// Bundled plugins first, then the installed ones in installation order. A plugin that
    /// does not load any more (file gone or damaged) is left out and reported in the second
    /// list. A damaged state file is an error for the whole call.
    #[allow(clippy::type_complexity)]
    pub fn load<M: PluginManifest>(
        &self,
    ) -> Result<(Vec<Plugin<M>>, Vec<(String, PluginError)>), PluginError> {
        let state = self.state()?;
        let mut plugins = Vec::new();
        let mut failed = Vec::new();
        for wasm in &self.bundled {
            match Plugin::<M>::compile(self.abi, self.api, wasm, &[], true) {
                Ok(mut p) => {
                    p.bundled = true;
                    // A bundled plugin has no grant: it asks for no network in v1.
                    p.granted_network = p.manifest.network().to_vec();
                    p.enabled = state
                        .plugins
                        .iter()
                        .find(|e| e.bundled && e.id == p.id())
                        .is_none_or(|e| e.enabled);
                    plugins.push(p);
                }
                Err(e) => failed.push(("bundled".to_owned(), e)),
            }
        }
        for entry in state.plugins.iter().filter(|e| !e.bundled) {
            let loaded = std::fs::read(self.module_path(&entry.id))
                .map_err(io)
                .and_then(|wasm| {
                    Plugin::<M>::compile(self.abi, self.api, &wasm, &entry.granted, entry.enabled)
                })
                .and_then(|p| {
                    if p.id() == entry.id {
                        Ok(p)
                    } else {
                        Err(PluginError::Invalid(format!(
                            "{}.wasm contains plugin {}",
                            entry.id,
                            p.id()
                        )))
                    }
                });
            match loaded {
                Ok(p) => plugins.push(p),
                Err(e) => failed.push((entry.id.clone(), e)),
            }
        }
        Ok((plugins, failed))
    }

    /// Checks `wasm` without installing it.
    pub fn inspect<M: PluginManifest>(&self, wasm: &[u8]) -> Result<M, PluginError> {
        let manifest = crate::manifest::read::<M>(&self.abi, self.api, wasm)?;
        crate::runtime::compile(&self.abi, wasm).map_err(PluginError::Invalid)?;
        Ok(manifest)
    }

    /// Installs or updates a plugin and grants what its manifest requests (the shell asked
    /// the user first). An update keeps the plugin's place in the order and whether it was
    /// on; a new plugin is on. A module with the id of a bundled plugin is refused.
    pub fn install<M: PluginManifest>(&self, wasm: &[u8]) -> Result<Plugin<M>, PluginError> {
        let manifest = self.inspect::<M>(wasm)?;
        let id = manifest.id().to_owned();
        for bundled in &self.bundled {
            if let Ok(b) = crate::manifest::read::<M>(&self.abi, self.api, bundled)
                && b.id() == id
            {
                return Err(PluginError::Invalid(format!(
                    "{id} is a bundled plugin; it cannot be replaced"
                )));
            }
        }
        let mut state = self.state()?;
        let enabled = state
            .plugins
            .iter()
            .find(|e| !e.bundled && e.id == id)
            .is_none_or(|e| e.enabled);
        let plugin = Plugin::<M>::compile(self.abi, self.api, wasm, manifest.network(), enabled)?;
        write_atomic(&self.module_path(&id), wasm).map_err(io)?;
        let entry = Entry {
            id: id.clone(),
            version: manifest.version().to_owned(),
            granted: manifest.network().to_vec(),
            enabled,
            bundled: false,
        };
        match state.plugins.iter_mut().find(|e| !e.bundled && e.id == id) {
            Some(e) => *e = entry,
            None => state.plugins.push(entry),
        }
        self.save_state(&state)?;
        Ok(plugin)
    }

    /// What the state file records, in order (nothing is compiled).
    pub fn records(&self) -> Result<Vec<InstalledRecord>, PluginError> {
        Ok(self
            .state()?
            .plugins
            .into_iter()
            .map(|e| InstalledRecord {
                id: e.id,
                version: e.version,
                granted: e.granted,
                enabled: e.enabled,
                bundled: e.bundled,
            })
            .collect())
    }

    /// The installed module `id`, unchanged.
    pub fn module_bytes(&self, id: &str) -> Option<Vec<u8>> {
        if !crate::manifest::valid_id(id) {
            return None;
        }
        std::fs::read(self.module_path(id)).ok()
    }

    /// A small file next to the plugins, or `None`.
    pub fn read_aux(&self, name: &str) -> Option<Vec<u8>> {
        if !aux_name(name) {
            return None;
        }
        std::fs::read(self.dir.join(name)).ok()
    }

    /// Writes a small file next to the plugins (atomically).
    pub fn write_aux(&self, name: &str, bytes: &[u8]) -> Result<(), PluginError> {
        if !aux_name(name) {
            return Err(io(format!(
                "{name:?} is not a file name of the plugin folder"
            )));
        }
        write_atomic(&self.dir.join(name), bytes).map_err(io)
    }

    /// Removes an installed plugin and its module. Bundled plugins stay.
    pub fn remove(&self, id: &str) -> Result<(), PluginError> {
        let mut state = self.state()?;
        let before = state.plugins.len();
        state.plugins.retain(|e| e.bundled || e.id != id);
        if state.plugins.len() == before {
            return Err(PluginError::NotFound(id.to_owned()));
        }
        self.save_state(&state)?;
        if crate::manifest::valid_id(id) {
            match std::fs::remove_file(self.module_path(id)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io(e)),
                _ => {}
            }
        }
        Ok(())
    }

    /// Switches a plugin on or off, bundled or installed. `bundled_ids` is not needed: a
    /// bundled plugin without an entry gets one.
    pub fn set_enabled<M: PluginManifest>(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<(), PluginError> {
        let mut state = self.state()?;
        if let Some(e) = state.plugins.iter_mut().find(|e| !e.bundled && e.id == id) {
            e.enabled = enabled;
            return self.save_state(&state);
        }
        let bundled = self.bundled.iter().find_map(|wasm| {
            crate::manifest::read::<M>(&self.abi, self.api, wasm)
                .ok()
                .filter(|m| m.id() == id)
        });
        let Some(manifest) = bundled else {
            return Err(PluginError::NotFound(id.to_owned()));
        };
        match state.plugins.iter_mut().find(|e| e.bundled && e.id == id) {
            Some(e) => e.enabled = enabled,
            None => state.plugins.push(Entry {
                id: id.to_owned(),
                version: manifest.version().to_owned(),
                granted: Vec::new(),
                enabled,
                bundled: true,
            }),
        }
        self.save_state(&state)
    }
}

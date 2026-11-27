// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors
// Copyright 2026 The Tailor Kit Authors

//! The manifest: UTF-8 JSON in the custom section named by the [`Abi`], checked before
//! anything of the plugin runs. The product's manifest type implements [`PluginManifest`]
//! for the fields every host checks; the rest of its shape is the product's.

use serde::de::DeserializeOwned;

use crate::permissions::valid_network_entry;
use crate::{Abi, PluginError};

/// Largest manifest accepted.
const MAX_MANIFEST_BYTES: usize = 256 * 1024;

/// The fields of a manifest every host needs.
pub trait PluginManifest: DeserializeOwned + Clone + std::fmt::Debug {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    /// The plugin API version it was built for.
    fn api(&self) -> u32;
    /// Network hosts it asks for (see [`valid_network_entry`]).
    fn network(&self) -> &[String];
    /// The product's own rules, run after the common ones.
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Reads the manifest section of `wasm` and checks it. Does not validate the code.
pub fn read<M: PluginManifest>(
    abi: &Abi,
    supported_api: u32,
    wasm: &[u8],
) -> Result<M, PluginError> {
    let invalid = |m: &str| PluginError::Invalid(m.to_owned());
    let mut found: Option<&[u8]> = None;
    for (name, data) in custom_sections(wasm)? {
        if name == abi.manifest_section.as_bytes() {
            if found.is_some() {
                return Err(PluginError::Invalid(format!(
                    "more than one {} section",
                    abi.manifest_section
                )));
            }
            found = Some(data);
        }
    }
    let data = found.ok_or_else(|| {
        PluginError::Invalid(format!(
            "no {} section with a manifest",
            abi.manifest_section
        ))
    })?;
    if data.len() > MAX_MANIFEST_BYTES {
        return Err(invalid("the manifest is too large"));
    }
    let text = std::str::from_utf8(data).map_err(|_| invalid("the manifest is not UTF-8"))?;
    let manifest: M = serde_json::from_str(text)
        .map_err(|e| PluginError::Invalid(format!("invalid manifest: {e}")))?;
    check(&manifest, supported_api)?;
    Ok(manifest)
}

/// The common rules, then the product's.
pub fn check<M: PluginManifest>(m: &M, supported_api: u32) -> Result<(), PluginError> {
    let invalid = |m: String| Err(PluginError::Invalid(m));
    if m.api() < 1 || m.api() > supported_api {
        return Err(PluginError::ApiVersion {
            needs: m.api(),
            runs: supported_api,
        });
    }
    if !valid_id(m.id()) {
        return invalid(format!(
            "invalid id {:?}: use [a-z0-9][a-z0-9-]*, at most 64 characters",
            m.id()
        ));
    }
    if m.name().trim().is_empty() {
        return invalid("the manifest has no name".into());
    }
    if m.version().trim().is_empty() {
        return invalid("the manifest has no version".into());
    }
    if let Some(bad) = m.network().iter().find(|e| !valid_network_entry(e)) {
        return invalid(format!("invalid network permission {bad:?}"));
    }
    m.check().map_err(PluginError::Invalid)
}

pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A custom section: name and data.
type Section<'a> = (&'a [u8], &'a [u8]);

/// `(name, data)` of every custom section, from the module's binary layout: the 8-byte
/// header, then sections of `id: u8, size: leb128 u32, payload`.
fn custom_sections(wasm: &[u8]) -> Result<Vec<Section<'_>>, PluginError> {
    let invalid = || PluginError::Invalid("not a WebAssembly module".into());
    if wasm.len() < 8 || &wasm[..4] != b"\0asm" || wasm[4..8] != [1, 0, 0, 0] {
        return Err(invalid());
    }
    let mut out = Vec::new();
    let mut pos = 8;
    while pos < wasm.len() {
        let id = wasm[pos];
        pos += 1;
        let size = leb128(wasm, &mut pos).ok_or_else(invalid)? as usize;
        let end = pos
            .checked_add(size)
            .filter(|&e| e <= wasm.len())
            .ok_or_else(invalid)?;
        if id == 0 {
            let mut p = pos;
            let len = leb128(&wasm[..end], &mut p).ok_or_else(invalid)? as usize;
            let name_end = p
                .checked_add(len)
                .filter(|&e| e <= end)
                .ok_or_else(invalid)?;
            out.push((&wasm[p..name_end], &wasm[name_end..end]));
        }
        pos = end;
    }
    Ok(out)
}

fn leb128(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    let mut result: u32 = 0;
    for shift in (0..35).step_by(7) {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        result |= u32::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(result);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        for ok in ["odata", "jwt-inspector", "a", "0x", &"a".repeat(64)] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "-a", "A", "a_b", "a.b", "ä", &"a".repeat(65)] {
            assert!(!valid_id(bad), "{bad}");
        }
    }
}

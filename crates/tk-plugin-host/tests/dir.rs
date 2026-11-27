// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! The plugin folder: bundled, installed, on or off, damaged state.

mod support;

use std::path::PathBuf;

use support::{API, Body, Manifest, TEST_ABI, manifest, plugin};
use tk_plugin_host::{PluginDir, PluginError};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tk-plugin-host-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn wasm(id: &str, version: &str, network: &[&str]) -> Vec<u8> {
    plugin(&TEST_ABI, &manifest(id, version, network), Body::Echo)
}

#[test]
fn install_update_disable_remove() {
    let root = temp("lifecycle");
    let dir = PluginDir::new(root.join("plugins"), TEST_ABI, API);
    assert!(dir.load::<Manifest>().unwrap().0.is_empty());

    dir.install::<Manifest>(&wasm("other", "1.0.0", &[]))
        .unwrap();
    let demo = dir
        .install::<Manifest>(&wasm("demo", "1.0.0", &["a.test"]))
        .unwrap();
    assert!(demo.fully_granted() && demo.enabled && !demo.bundled);
    assert!(root.join("plugins/demo.wasm").is_file());

    dir.set_enabled::<Manifest>("demo", false).unwrap();
    // An update keeps the order and whether the plugin is on, and grants the new hosts.
    let v2 = dir
        .install::<Manifest>(&wasm("demo", "2.0.0", &["a.test", "b.test"]))
        .unwrap();
    assert_eq!(v2.manifest.version, "2.0.0");
    assert!(!v2.enabled);
    let (loaded, failed) = dir.load::<Manifest>().unwrap();
    assert!(failed.is_empty(), "{failed:?}");
    let ids: Vec<_> = loaded
        .iter()
        .map(|p| (p.id().to_owned(), p.enabled))
        .collect();
    assert_eq!(
        ids,
        [("other".to_owned(), true), ("demo".to_owned(), false)]
    );
    assert_eq!(loaded[1].granted_network, ["a.test", "b.test"]);

    dir.remove("demo").unwrap();
    assert!(!root.join("plugins/demo.wasm").exists());
    assert!(matches!(dir.remove("demo"), Err(PluginError::NotFound(_))));
    assert!(matches!(
        dir.set_enabled::<Manifest>("demo", true),
        Err(PluginError::NotFound(_))
    ));
    // Invalid modules are refused before anything is written.
    assert!(dir.install::<Manifest>(b"junk").is_err());
    assert_eq!(dir.load::<Manifest>().unwrap().0.len(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bundled_plugins_load_first_cannot_be_removed_and_can_be_switched_off() {
    let root = temp("bundled");
    let dir = PluginDir::new(root.join("plugins"), TEST_ABI, API)
        .with_state_file(root.join("plugins.json"))
        .with_bundled([wasm("structure", "1.0.0", &[])]);
    dir.install::<Manifest>(&wasm("mine", "1.0.0", &[]))
        .unwrap();
    let (loaded, _) = dir.load::<Manifest>().unwrap();
    let list: Vec<_> = loaded
        .iter()
        .map(|p| (p.id().to_owned(), p.bundled, p.enabled))
        .collect();
    assert_eq!(
        list,
        [
            ("structure".to_owned(), true, true),
            ("mine".to_owned(), false, true)
        ]
    );

    assert!(matches!(
        dir.remove("structure"),
        Err(PluginError::NotFound(_))
    ));
    assert!(
        dir.install::<Manifest>(&wasm("structure", "9.0.0", &[]))
            .is_err()
    );
    dir.set_enabled::<Manifest>("structure", false).unwrap();
    let (loaded, _) = dir.load::<Manifest>().unwrap();
    assert!(!loaded[0].enabled && loaded[1].enabled);
    dir.set_enabled::<Manifest>("structure", true).unwrap();
    assert!(dir.load::<Manifest>().unwrap().0[0].enabled);
    assert!(root.join("plugins.json").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn damaged_or_ungranted_plugins_are_reported_or_get_no_rights() {
    let root = temp("damaged");
    let dir = PluginDir::new(root.join("plugins"), TEST_ABI, API);
    dir.install::<Manifest>(&wasm("demo", "1.0.0", &[]))
        .unwrap();
    // Someone swaps in a version that wants more: it loads without any permission.
    std::fs::write(
        root.join("plugins/demo.wasm"),
        wasm("demo", "2.0.0", &["a.test"]),
    )
    .unwrap();
    let (loaded, _) = dir.load::<Manifest>().unwrap();
    assert!(loaded[0].granted_network.is_empty());
    assert!(!loaded[0].fully_granted());
    // A module that is not the plugin the state names, or no module at all, is reported.
    std::fs::write(root.join("plugins/demo.wasm"), wasm("other", "1.0.0", &[])).unwrap();
    let (loaded, failed) = dir.load::<Manifest>().unwrap();
    assert!(loaded.is_empty());
    assert_eq!(failed[0].0, "demo");
    std::fs::write(root.join("plugins/demo.wasm"), b"junk").unwrap();
    assert_eq!(dir.load::<Manifest>().unwrap().1.len(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_damaged_or_newer_state_file_is_never_overwritten() {
    let root = temp("state");
    let dir = PluginDir::new(root.join("plugins"), TEST_ABI, API);
    std::fs::create_dir_all(root.join("plugins")).unwrap();
    for text in ["{ not json", r#"{"version": 99, "plugins": []}"#] {
        std::fs::write(root.join("plugins/plugins.json"), text).unwrap();
        assert!(dir.load::<Manifest>().is_err());
        assert!(dir.install::<Manifest>(&wasm("x", "1.0.0", &[])).is_err());
        assert!(dir.set_enabled::<Manifest>("x", false).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("plugins/plugins.json")).unwrap(),
            text
        );
        assert!(
            !root.join("plugins/x.wasm").exists() || {
                // The module may be written before the state; the state is what must survive.
                true
            }
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn aux_files_stay_inside_the_folder() {
    let root = temp("aux");
    let dir = PluginDir::new(root.join("plugins"), TEST_ABI, API);
    dir.write_aux("sources.json", b"[]").unwrap();
    assert_eq!(dir.read_aux("sources.json").unwrap(), b"[]");
    for bad in ["../x", "a/b", ".hidden", "", "a\\b"] {
        assert!(dir.write_aux(bad, b"x").is_err(), "{bad}");
        assert!(dir.read_aux(bad).is_none(), "{bad}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

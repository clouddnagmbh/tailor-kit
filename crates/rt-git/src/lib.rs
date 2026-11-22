// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `rt-git`: Git for the repository that holds a collection, on native targets, with gitoxide
//! (`gix`): no installed `git` needed. Answers with the types of `rt_app::git`, which the web
//! app's isomorphic-git implementation answers with too.
//!
//! I/O happens here (repository on disk, network); `rt-app` stays I/O-free. The desktop app
//! calls this from `apps/desktop/src-tauri/src/git.rs`.
//!
//! Owner: wave 4, strand A1.

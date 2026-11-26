// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! `GitApi.resolveText` (wave 15, strand D): the result of the 3-way text conflict view. The
//! text becomes the file in the working tree and the conflict is marked resolved in the index;
//! the next commit concludes the merge, cherry-pick or revert.
//!
//! The caller refuses while the app has unsaved changes, as for every resolve.

use gix::bstr::BStr;
use gix::index::entry::{Flags, Mode, Stage};

use crate::repo::Repo;
use crate::util::{Blob, Context, Result, fail};
use crate::worktree::{Probe, stat_of};

impl Repo {
    /// Writes `text` as the resolution of the conflicted file `path` (scope-relative, or
    /// repository-relative with a leading `/`).
    pub fn resolve_text(&self, path: &str, text: &str) -> Result<()> {
        let repo_path = self.repo_path(path)?;
        let mut index = self.load_index()?;
        let at: &BStr = repo_path.as_ref();
        let conflicted = index
            .entries()
            .iter()
            .any(|e| e.stage() != Stage::Unconflicted && e.path(&index) == at);
        if !conflicted {
            return Err(fail(format!("{path} has no conflict")));
        }
        // Keep the executable bit the sides agreed on, else a plain file.
        let mode = [Stage::Ours, Stage::Theirs, Stage::Base]
            .into_iter()
            .find_map(|s| index.entry_by_path_and_stage(at, s).map(|e| e.mode))
            .filter(|m| *m == Mode::FILE_EXECUTABLE)
            .unwrap_or(Mode::FILE);
        let id = self
            .repo
            .write_blob(text.as_bytes())
            .ctx("cannot write the resolved file")?
            .detach();
        index.remove_entries(|_, p, _| p == at);
        let file = Probe::new(self)?.write(at, Blob { mode, id })?;
        index.dangerously_push_entry(stat_of(&file), id, Flags::empty(), mode, at);
        self.write_index(&mut index)
    }
}

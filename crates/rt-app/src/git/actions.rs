// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Git actions for the wizards (wave 14, strand G): one [`GitAction`] per thing a user can ask
//! for, a [`GitPlan`] that says in plain words what it would do *before* it runs, and a
//! [`GitActionResult`] with an undo entry afterwards.
//!
//! Flow: the wizard builds a [`GitAction`], calls `GitApi.plan` (reads only), shows the plan
//! (ref moves, files touched, merge preview, backup), and on confirmation `GitApi.apply` with
//! the same action. `apply` plans again and refuses with kind `git` and the plan's
//! [`GitBlock::message`] when it is blocked now. Natively in `rt-git` (gitoxide), in the browser
//! with isomorphic-git; both answer with these types.
//!
//! Rules, the same for every shell:
//! - Whatever moves a ref or changes the working tree writes a backup first:
//!   `refs/requesttailor/backup/<unix>/<name>` ([`backup_ref`], as the wave-12 secret rescue),
//!   and an [`GitUndoEntry`] to the repository's action journal (`.git/requesttailor/actions.jsonl`,
//!   never committed, at most [`UNDO_KEEP`] entries). `GitApi.undo` puts the refs (and, if `HEAD`
//!   moved, the working tree) back where the entry says; it is refused once the refs moved since.
//! - Working-tree changes are refused while the app has unsaved changes
//!   ([`GitBlockReason::UnsavedChanges`]); afterwards every collection of the repository reloads.
//! - Merge, cherry-pick and revert merge semantically where a file is a collection file
//!   (wave 12, `rt_format::merge`) and line by line everywhere else; conflicts end in the
//!   wave-12 conflict views (`MergeApi`, `GitApi.conflict`/`resolve`).
//! - No force push and no history rewrite of published commits from the app. A diverged branch
//!   offers merge or "take theirs" (reset with backup). Since wave 15, [`GitAction::Amend`] and
//!   [`GitAction::Rewrite`] change unpublished history only: a commit they would replace that is
//!   on a remote-tracking branch blocks them ([`GitBlockReason::Published`]).
//! - Generic: nothing assumes the repository is a collection. Paths are repository-relative with
//!   a leading `/`. New actions are new variants (wave 15 adds more), so the UI must ignore
//!   unknown [`GitPlan`] fields and treat an unknown action as unsupported.
//!
//! The types and rules live in the Tailor Kit (`tk_git_types::actions`); this module keeps the
//! old paths and adds RequestTailor's backup refs.

pub use tk_git_types::actions::*;

/// Prefix of the backup refs, the same as the secret rescue's.
pub use crate::rescue::BACKUP_PREFIX;

/// The backup ref for `name` (a short branch or tag name, or `HEAD`) at `unix` seconds.
pub fn backup_ref(unix: i64, name: &str) -> String {
    super::REF_NAMESPACE.backup_ref(unix, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backups_share_the_rescue_prefix() {
        assert_eq!(
            backup_ref(1_700_000_000, "main"),
            "refs/requesttailor/backup/1700000000/main"
        );
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Repositories (wave 15, strand A): what a sidebar shows per repository, and the operation a
//! repository is in the middle of. Which repositories a product lists, and how it names them,
//! is the product's business.

use serde::{Deserialize, Serialize};

/// What the sidebar shows next to a repository, asked for per repository (cheap: branch,
/// ahead/behind against the upstream, the number of changed files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRepoSummary {
    /// The product's id of the repository.
    pub id: String,
    /// `None`: detached `HEAD` or no commit yet.
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Changed, new and conflicted files of the whole working tree.
    pub changes: u32,
    /// A merge, cherry-pick, revert or rebase is half done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<GitOperation>,
}

/// A Git operation that stopped half way (conflicts, or started on the command line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitOperation {
    Merge,
    CherryPick,
    Revert,
    /// A rebase started outside the app (`.git/rebase-merge`, `.git/rebase-apply`); the app
    /// offers only to abort it or to finish it on the command line.
    Rebase,
}

/// The sidebar's summary of a repository from the status of its whole working tree
/// (`operation`: what the shell found half done; a `merging` status without one is a merge).
pub fn summarize(
    id: &str,
    status: &crate::GitStatus,
    operation: Option<GitOperation>,
) -> GitRepoSummary {
    GitRepoSummary {
        id: id.to_owned(),
        branch: status.branch.clone(),
        upstream: status.upstream.clone(),
        ahead: status.ahead,
        behind: status.behind,
        changes: u32::try_from(status.files.len()).unwrap_or(u32::MAX),
        operation: operation.or(status.merging.then_some(GitOperation::Merge)),
    }
}

/// The default folder name for a clone of `url`: its last path segment without `.git`
/// (`https://host/acme/orders.git` → `orders`, `/repos/api/` → `api`), `repository` if none.
pub fn clone_folder_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches(['/', '\\']);
    let last = trimmed.rsplit(['/', '\\', ':']).next().unwrap_or_default();
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() || name == "." || name == ".." {
        "repository".to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_folder_names_come_from_the_url() {
        assert_eq!(
            clone_folder_name("https://github.com/acme/orders.git"),
            "orders"
        );
        assert_eq!(clone_folder_name("https://gitlab.com/g/sub/api"), "api");
        assert_eq!(clone_folder_name("git@github.com:acme/web.git"), "web");
        assert_eq!(clone_folder_name("/repos/api/"), "api");
        assert_eq!(clone_folder_name("file:///tmp/x.git"), "x");
        assert_eq!(clone_folder_name(""), "repository");
        assert_eq!(clone_folder_name("https://host/.git"), "repository");
    }

    #[test]
    fn operations_are_kebab_case() {
        assert_eq!(
            serde_json::to_value(GitOperation::CherryPick).unwrap(),
            "cherry-pick"
        );
    }
}

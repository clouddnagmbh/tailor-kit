// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Choosing lines of a diff (wave 15, strand F). A [`GitHunkSelection`] indexes the hunks and
//! lines of the diff `rt_app::git::text_diff` returns; this module maps them back onto the
//! two texts and builds the text with exactly the chosen changes applied (or, inverted, with
//! exactly the chosen changes taken back). It is what `git add -p`, `git reset -p` and
//! `git checkout -p` compute, without writing a patch file.
//!
//! Unchosen removals stay as context and unchosen additions are dropped, the way Git treats
//! the lines of a hunk edited with `e`.

use std::collections::BTreeSet;

use rt_app::git::DIFF_CONTEXT;
use rt_app::git::index::GitHunkSelection;
use similar::{ChangeTag, TextDiff};

use crate::util::{Result, fail};

/// One changed line, by its place in the old or the new text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Removed(usize),
    Added(usize),
}

/// The chosen changes of `old` → `new`.
pub(crate) struct Selection {
    chosen: BTreeSet<Key>,
}

impl Selection {
    /// Maps `selection` (hunk and line indexes of `text_diff(old, new)`) onto the lines.
    /// Context lines in a selection are ignored; indexes that do not exist are refused.
    pub fn of(old: &str, new: &str, selection: &[GitHunkSelection]) -> Result<Self> {
        let diff = TextDiff::from_lines(old, new);
        let groups = diff.grouped_ops(DIFF_CONTEXT);
        let mut chosen = BTreeSet::new();
        for pick in selection {
            let group = groups
                .get(pick.hunk as usize)
                .ok_or_else(|| fail(format!("the diff has no hunk {}", pick.hunk)))?;
            let changes: Vec<_> = group.iter().flat_map(|op| diff.iter_changes(op)).collect();
            let wanted: Vec<usize> = match &pick.lines {
                None => (0..changes.len()).collect(),
                Some(lines) => lines.iter().map(|l| *l as usize).collect(),
            };
            for line in wanted {
                let change = changes
                    .get(line)
                    .ok_or_else(|| fail(format!("hunk {} has no line {line}", pick.hunk)))?;
                match (change.tag(), change.old_index(), change.new_index()) {
                    (ChangeTag::Delete, Some(i), _) => {
                        chosen.insert(Key::Removed(i));
                    }
                    (ChangeTag::Insert, _, Some(i)) => {
                        chosen.insert(Key::Added(i));
                    }
                    _ => {}
                }
            }
        }
        Ok(Self { chosen })
    }

    pub fn is_empty(&self) -> bool {
        self.chosen.is_empty()
    }

    /// `old` with the chosen changes applied (`invert` false), or `new` with the chosen
    /// changes taken back (`invert` true).
    pub fn render(&self, old: &str, new: &str, invert: bool) -> String {
        let diff = TextDiff::from_lines(old, new);
        let mut out = String::with_capacity(old.len().max(new.len()));
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Equal => out.push_str(change.value()),
                ChangeTag::Delete => {
                    let chosen = change
                        .old_index()
                        .is_some_and(|i| self.chosen.contains(&Key::Removed(i)));
                    // Applied: the line goes. Not applied: it stays.
                    if chosen == invert {
                        out.push_str(change.value());
                    }
                }
                ChangeTag::Insert => {
                    let chosen = change
                        .new_index()
                        .is_some_and(|i| self.chosen.contains(&Key::Added(i)));
                    if chosen != invert {
                        out.push_str(change.value());
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(hunk: u32, lines: Option<Vec<u32>>) -> GitHunkSelection {
        GitHunkSelection { hunk, lines }
    }

    #[test]
    fn a_whole_hunk_applies_all_its_changes() {
        let old = "a\nb\nc\n";
        let new = "a\nB\nc\nd\n";
        let sel = Selection::of(old, new, &[pick(0, None)]).unwrap();
        assert_eq!(sel.render(old, new, false), new);
        assert_eq!(sel.render(old, new, true), old);
    }

    #[test]
    fn chosen_lines_only() {
        let old = "a\nb\nc\n";
        let new = "a\nB\nc\nd\n";
        // Lines: a (context) -b +B c (context) +d
        let sel = Selection::of(old, new, &[pick(0, Some(vec![2]))]).unwrap();
        assert_eq!(sel.render(old, new, false), "a\nb\nB\nc\n");
        // Taking back only "+B": the unchosen removal of "b" and addition of "d" stay.
        assert_eq!(sel.render(old, new, true), "a\nc\nd\n");
        let only_removal = Selection::of(old, new, &[pick(0, Some(vec![1]))]).unwrap();
        assert_eq!(only_removal.render(old, new, false), "a\nc\n");
    }

    #[test]
    fn unknown_indexes_are_refused() {
        assert!(Selection::of("a\n", "b\n", &[pick(3, None)]).is_err());
        assert!(Selection::of("a\n", "b\n", &[pick(0, Some(vec![9]))]).is_err());
    }
}

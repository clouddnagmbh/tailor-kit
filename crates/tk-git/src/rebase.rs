// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Rebase as picks in memory (GitTailor wave 3, strand A): [`GitAction::Rebase`],
//! [`GitAction::RebaseContinue`], [`GitAction::RebaseSkip`], [`GitAction::PullRebase`] and the
//! state of a stopped rebase in `.git/rebase-merge/` ([`Repo::rebase_state`]). Types and rules:
//! [`tk_git_types::rebase`].
//!
//! The steps are replayed one by one: a pick merges the commit's change onto the commit before
//! it (Git's tree merge with rename tracking, as `git rebase` does it, and the product's file
//! merger for its files), `squash` and `fixup` fold into that commit, `drop` leaves the commit
//! out. A commit whose parent is the commit before it is kept as it is (same id, like
//! `git rebase` without `--force-rebase`); one that becomes empty on the new base is left out
//! (`--empty=drop`); one that was empty to begin with is kept. The plan runs every step in an
//! object store in memory (nothing in `.git` changes); the run writes the commits and moves the
//! branch once, at the end, with a backup ref and a journal entry.
//!
//! A step that stops (a conflict, `edit`, `reword` without a message) leaves the repository
//! the way `git rebase -i` leaves it: `HEAD` detached at the last commit made, the branch where
//! it was, conflict stages in the index and markers in the files, and the state in Git's own
//! files (`rebase-merge/{head-name,onto,orig-head,interactive,git-rebase-todo,done,msgnum,end,
//! stopped-sha,message,author-script,amend,current-fixups,message-squash,message-fixup,
//! drop_redundant_commits}`, `REBASE_HEAD`, `MERGE_MSG`, `SQUASH_MSG`). `git status` then says
//! "interactive rebase in progress", and `git rebase --continue`/`--skip`/`--abort` take over;
//! a rebase `git rebase -i` started is continued here the same way. Messages for steps that are
//! still to come (a `reword` or `squash` with its message) have no place in Git's todo; they
//! wait in the product's own folder (`<namespace>/rebase-messages`). The journal keeps all of
//! these files, so undo of a step brings the stopped rebase back exactly.
//!
//! Left out: `--rebase-merges` (merge commits of the branch are not replayed, as without the
//! option), `exec`, `break`, `label`, `reset`, `merge` and `update-ref` steps (a todo with them
//! is continued with `git rebase --continue`), the fork point of `git pull --rebase` (the merge
//! base is used), and dropping commits whose patch is upstream before replaying (they become
//! empty and are left out while replaying).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::index::entry::Stage;
use tk_git_types::actions::{GitAction, GitBlock, GitBlockReason, GitPlan, GitRefMove};
use tk_git_types::rebase::{
    GitRebasePlanStep, GitRebasePreview, GitRebaseState, GitRebaseStep, GitRebaseStepKind,
    GitRebaseStop,
};
use tk_git_types::{GitFile, GitFileState, GitIdentity, GitPullResult};

use crate::actions::{Ctx, Done, PlanExt, Run, block, plural_s, short};
use crate::branch_actions::{branch_full, diff_paths};
use crate::merge::{MergeKind, ThreeWay};
use crate::repo::Repo;
use crate::util::{Context, Result, fail, flatten};

/// Git's folder of an interactive rebase in progress.
pub const REBASE_MERGE: &str = "rebase-merge";
/// Files next to it that belong to the stopped rebase.
const SIDE_FILES: [&str; 3] = ["REBASE_HEAD", "SQUASH_MSG", "AUTO_MERGE"];
/// The product's own file with the messages of steps still to come.
const MESSAGES_FILE: &str = "rebase-messages";

// ---- messages ---------------------------------------------------------------------------------

/// Git's `--cleanup=whitespace`: trailing spaces off, blank lines at the start and end off,
/// runs of blank lines folded into one, a final newline.
pub(crate) fn clean_whitespace(text: &str) -> String {
    let mut out = String::new();
    let mut blank = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if blank {
            out.push('\n');
            blank = false;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Git's `--cleanup=strip`: comment lines (`#`) off, then [`clean_whitespace`].
fn clean_strip(text: &str) -> String {
    clean_strip_as(text, "#")
}

/// [`clean_strip`] with the comment string `comment` (`core.commentString`).
fn clean_strip_as(text: &str, comment: &str) -> String {
    let kept: Vec<&str> = text.lines().filter(|l| !l.starts_with(comment)).collect();
    clean_whitespace(&kept.join("\n"))
}

/// A message the user gave (as `git commit -m` takes it); `None` when nothing is left.
fn given(message: &Option<String>) -> Option<String> {
    message
        .as_deref()
        .map(clean_whitespace)
        .filter(|m| !m.is_empty())
}

/// `fixup!`, `squash!` or `amend!` at the start of a subject: the rest after it.
fn skip_fixupish(subject: &str) -> Option<&str> {
    ["fixup!", "squash!", "amend!"]
        .iter()
        .find_map(|p| subject.strip_prefix(p))
}

fn ordinal(n: usize) -> String {
    if n == 1 {
        "This is the 1st commit message:".into()
    } else {
        format!("This is the commit message #{n}:")
    }
}

/// A `squash`/`fixup` chain: the commit it folds into and the messages, as Git keeps them.
#[derive(Debug, Clone)]
struct Chain {
    /// The commit standing for the chain so far (`HEAD`; an interim commit after a fold).
    head: ObjectId,
    /// Parents and author of the commit the chain folds into.
    parents: Vec<ObjectId>,
    author: gix::actor::Signature,
    /// The first commit's message, then one per folded commit.
    first: String,
    folded: Vec<(GitRebaseStepKind, ObjectId, String)>,
    /// The latest message given with a `squash` of the chain.
    given: Option<String>,
    /// The line whose result the chain's commit is (unknown when resumed).
    owner: Option<usize>,
}

impl Chain {
    /// Git's `message-squash`.
    fn squash_text(&self) -> String {
        let n = self.folded.len() + 1;
        let mut out = format!(
            "# This is a combination of {n} commits.\n# {}\n\n{}",
            ordinal(1),
            self.first
        );
        for (i, (kind, _, msg)) in self.folded.iter().enumerate() {
            let nth = i + 2;
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if *kind == GitRebaseStepKind::Fixup {
                out.push_str(&format!(
                    "\n# The commit message #{nth} will be skipped:\n\n"
                ));
                for line in msg.lines() {
                    out.push_str("# ");
                    out.push_str(line);
                    out.push('\n');
                }
            } else {
                out.push_str(&format!("\n# {}\n\n", ordinal(nth)));
                let mut lines = msg.lines();
                if let Some(first) = lines.next() {
                    if skip_fixupish(first).is_some() {
                        out.push_str("# ");
                    }
                    out.push_str(first);
                    out.push('\n');
                }
                for line in lines {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out
    }

    fn only_fixups(&self) -> bool {
        self.folded
            .iter()
            .all(|(k, _, _)| *k == GitRebaseStepKind::Fixup)
    }

    /// The message of the chain's commit: the latest given one, the first commit's when only
    /// fixups follow it, else the combination without comments.
    fn message(&self) -> String {
        if let Some(m) = &self.given {
            return m.clone();
        }
        if self.only_fixups() {
            return self.first.clone();
        }
        clean_strip(&self.squash_text())
    }

    /// Git's `current-fixups`.
    fn fixups_text(&self) -> String {
        self.folded
            .iter()
            .map(|(k, id, _)| format!("{} {id}", word(*k)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---- the todo ---------------------------------------------------------------------------------

fn word(kind: GitRebaseStepKind) -> &'static str {
    match kind {
        GitRebaseStepKind::Pick => "pick",
        GitRebaseStepKind::Reword => "reword",
        GitRebaseStepKind::Edit => "edit",
        GitRebaseStepKind::Squash => "squash",
        GitRebaseStepKind::Fixup => "fixup",
        GitRebaseStepKind::Drop => "drop",
    }
}

/// One line of a todo as Git writes it.
#[derive(Debug, Clone)]
enum TodoLine {
    Step(GitRebaseStep),
    /// `fixup -C`/`-c`: fold, keep this commit's message (without its `amend!` line).
    FixupC(ObjectId),
    /// `exec`, `break`, `label` … (not run here).
    Other(String),
}

fn parse_todo(repo: &gix::Repository, text: &str) -> Vec<TodoLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line == "noop" {
            continue;
        }
        let mut words = line.split_whitespace();
        let cmd = words.next().unwrap_or("");
        let kind = match cmd {
            "p" | "pick" => Some(GitRebaseStepKind::Pick),
            "r" | "reword" => Some(GitRebaseStepKind::Reword),
            "e" | "edit" => Some(GitRebaseStepKind::Edit),
            "s" | "squash" => Some(GitRebaseStepKind::Squash),
            "f" | "fixup" => Some(GitRebaseStepKind::Fixup),
            "d" | "drop" => Some(GitRebaseStepKind::Drop),
            _ => None,
        };
        let Some(kind) = kind else {
            out.push(TodoLine::Other(line.to_owned()));
            continue;
        };
        let mut arg = words.next().unwrap_or("");
        let mut fixup_c = false;
        if kind == GitRebaseStepKind::Fixup && (arg == "-C" || arg == "-c") {
            fixup_c = true;
            arg = words.next().unwrap_or("");
        }
        let id = repo
            .rev_parse_single(arg)
            .ok()
            .and_then(|i| i.object().ok())
            .and_then(|o| o.peel_to_commit().ok())
            .map(|c| c.id);
        match id {
            Some(id) if fixup_c => out.push(TodoLine::FixupC(id)),
            Some(id) => out.push(TodoLine::Step(GitRebaseStep {
                kind,
                commit: id.to_string(),
                message: None,
            })),
            None => out.push(TodoLine::Other(line.to_owned())),
        }
    }
    out
}

/// The messages kept for steps still to come, by line (`<kind> <commit>`).
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct Messages {
    orig_head: String,
    messages: Vec<(String, String)>,
}

fn line_key(step: &GitRebaseStep) -> String {
    format!("{} {}", word(step.kind), step.commit)
}

// ---- the replay -------------------------------------------------------------------------------

/// A line of the todo the engine runs.
#[derive(Debug, Clone)]
struct Line {
    kind: GitRebaseStepKind,
    commit: ObjectId,
    message: Option<String>,
    /// `fixup -C`: the chain takes this commit's message.
    take_message: bool,
}

impl Line {
    fn step(&self) -> GitRebaseStep {
        GitRebaseStep {
            kind: self.kind,
            commit: self.commit.to_string(),
            message: self.message.clone(),
        }
    }

    fn folds(&self) -> bool {
        matches!(
            self.kind,
            GitRebaseStepKind::Squash | GitRebaseStepKind::Fixup
        )
    }
}

/// Why the replay stopped, at which line.
#[derive(Debug, Clone)]
enum Halt {
    Conflict { at: usize, paths: Vec<String> },
    Edit { at: usize },
    Message { at: usize },
}

impl Halt {
    fn at(&self) -> usize {
        match self {
            Halt::Conflict { at, .. } | Halt::Edit { at } | Halt::Message { at } => *at,
        }
    }
}

/// Where the replay is.
struct Replayed {
    current: ObjectId,
    chain: Option<Chain>,
    steps: Vec<GitRebasePlanStep>,
}

/// Replays lines with objects written to `repo` (in memory for a plan).
struct Engine<'r> {
    repo: &'r Repo,
    committer: gix::actor::Signature,
    /// Commits go through the product's signer (a run and a dry run; never a plan).
    sign: bool,
}

struct Picked {
    tree: ObjectId,
    parent: Option<ObjectId>,
    parents: Vec<ObjectId>,
    parent_tree: ObjectId,
    author: gix::actor::Signature,
    message: String,
}

impl Engine<'_> {
    fn read(&self, id: ObjectId) -> Result<Picked> {
        let commit = self.repo.repo.find_commit(id).ctx("cannot read a commit")?;
        let decoded = commit.decode().ctx("cannot read a commit")?;
        let author: gix::actor::Signature = decoded
            .author()
            .ctx("cannot read a commit")?
            .to_owned()
            .ctx("cannot read a commit")?;
        let parents: Vec<ObjectId> = decoded.parents().collect();
        let parent = parents.first().copied();
        let parent_tree = match parent {
            Some(p) => self.repo.commit_tree(p)?,
            None => ObjectId::empty_tree(self.repo.repo.object_hash()),
        };
        Ok(Picked {
            tree: decoded.tree(),
            parent,
            parents,
            parent_tree,
            author,
            message: decoded.message.to_str_lossy().into_owned(),
        })
    }

    fn commit(
        &self,
        tree: ObjectId,
        parents: Vec<ObjectId>,
        author: gix::actor::Signature,
        message: &str,
    ) -> Result<ObjectId> {
        let commit = gix::objs::Commit {
            tree,
            parents: parents.into(),
            author,
            committer: self.committer.clone(),
            encoding: None,
            message: message.into(),
            extra_headers: Vec::new(),
        };
        if self.sign {
            return self.repo.write_commit_object(commit);
        }
        Ok(self
            .repo
            .repo
            .write_object(&commit)
            .ctx("cannot write a commit")?
            .detach())
    }

    /// The change of `commit` (base: its first parent) merged onto `ours`: the tree, and the
    /// files that conflict.
    fn merge(
        &self,
        ours: ObjectId,
        commit: ObjectId,
        p: &Picked,
    ) -> Result<(ObjectId, Vec<String>)> {
        let ours_tree = self.repo.commit_tree(ours)?;
        if ours_tree == p.parent_tree {
            return Ok((p.tree, Vec::new()));
        }
        if p.tree == p.parent_tree {
            return Ok((ours_tree, Vec::new()));
        }
        let label = label(self.repo, commit);
        let tw = ThreeWay {
            kind: MergeKind::CherryPick,
            base: p.parent,
            ours,
            theirs: commit,
            label: &label,
        };
        let computed = self.repo.compute_merge_with(&self.repo.repo, &tw, true)?;
        let (tree, texts, other) = self.repo.pick_outcome(computed)?;
        let mut conflicts: Vec<String> = texts.into_iter().map(|c| c.path).chain(other).collect();
        conflicts.sort();
        conflicts.dedup();
        Ok((tree, conflicts))
    }

    /// Ends the chain: its commit with the chain's message.
    fn end_chain(&self, r: &mut Replayed) -> Result<()> {
        let Some(chain) = r.chain.take() else {
            return Ok(());
        };
        let message = chain.message();
        let head = self.read(chain.head)?;
        let id = if head.message == message {
            chain.head
        } else {
            self.commit(
                head.tree,
                chain.parents.clone(),
                chain.author.clone(),
                &message,
            )?
        };
        r.current = id;
        if let Some(owner) = chain.owner
            && let Some(s) = r.steps.get_mut(owner)
        {
            s.result = Some(id.to_string());
        }
        Ok(())
    }

    /// A chain folding into `r.current`.
    fn chain_on(&self, r: &Replayed, owner: Option<usize>) -> Result<Chain> {
        let head = self.read(r.current)?;
        Ok(Chain {
            head: r.current,
            parents: head.parents,
            author: head.author,
            first: head.message,
            folded: Vec::new(),
            given: None,
            owner,
        })
    }

    /// Runs `lines[from..]`; `Some` when a line stops.
    fn replay(&self, lines: &[Line], from: usize, r: &mut Replayed) -> Result<Option<Halt>> {
        for (i, line) in lines.iter().enumerate().skip(from) {
            if !line.folds() {
                self.end_chain(r)?;
            }
            match line.kind {
                GitRebaseStepKind::Drop => {}
                GitRebaseStepKind::Pick | GitRebaseStepKind::Reword | GitRebaseStepKind::Edit => {
                    let p = self.read(line.commit)?;
                    let reword = if line.kind == GitRebaseStepKind::Reword {
                        given(&line.message)
                    } else {
                        None
                    };
                    let new = if p.parent == Some(r.current) {
                        match &reword {
                            Some(m) => {
                                self.commit(p.tree, p.parents.clone(), p.author.clone(), m)?
                            }
                            None => line.commit,
                        }
                    } else {
                        let (tree, conflicts) = self.merge(r.current, line.commit, &p)?;
                        if !conflicts.is_empty() {
                            r.steps[i].conflicts = conflicts.clone();
                            return Ok(Some(Halt::Conflict {
                                at: i,
                                paths: conflicts,
                            }));
                        }
                        if tree == self.repo.commit_tree(r.current)? && p.tree != p.parent_tree {
                            r.steps[i].empty = true;
                            continue;
                        }
                        let message = reword.clone().unwrap_or_else(|| p.message.clone());
                        self.commit(tree, vec![r.current], p.author.clone(), &message)?
                    };
                    r.current = new;
                    r.steps[i].result = Some(new.to_string());
                    if line.kind == GitRebaseStepKind::Edit {
                        return Ok(Some(Halt::Edit { at: i }));
                    }
                    if line.kind == GitRebaseStepKind::Reword && reword.is_none() {
                        return Ok(Some(Halt::Message { at: i }));
                    }
                }
                GitRebaseStepKind::Squash | GitRebaseStepKind::Fixup => {
                    if r.chain.is_none() {
                        let current = r.current.to_string();
                        let owner = r
                            .steps
                            .iter()
                            .rposition(|s| s.result.as_deref() == Some(current.as_str()));
                        r.chain = Some(self.chain_on(r, owner)?);
                    }
                    let p = self.read(line.commit)?;
                    let chain_head = r.chain.as_ref().map_or(r.current, |c| c.head);
                    let (tree, conflicts) = self.merge(chain_head, line.commit, &p)?;
                    let chain = r.chain.as_mut().expect("started above");
                    chain
                        .folded
                        .push((line.kind, line.commit, p.message.clone()));
                    if line.take_message {
                        let body =
                            clean_strip(&p.message.lines().skip(1).collect::<Vec<_>>().join("\n"));
                        chain.given = Some(if body.is_empty() {
                            p.message.clone()
                        } else {
                            body
                        });
                    }
                    if line.kind == GitRebaseStepKind::Squash
                        && let Some(m) = given(&line.message)
                    {
                        chain.given = Some(m);
                    }
                    if !conflicts.is_empty() {
                        r.steps[i].conflicts = conflicts.clone();
                        return Ok(Some(Halt::Conflict {
                            at: i,
                            paths: conflicts,
                        }));
                    }
                    let message = chain.message();
                    let (parents, author) = (chain.parents.clone(), chain.author.clone());
                    let interim = self.commit(tree, parents, author, &message)?;
                    if let Some(c) = r.chain.as_mut() {
                        c.head = interim;
                    }
                    r.current = interim;
                }
            }
        }
        self.end_chain(r)?;
        Ok(None)
    }
}

fn label(repo: &Repo, commit: ObjectId) -> String {
    format!("{} ({})", short(&commit), repo.subject(commit))
}

/// `true` when `name` matches `pattern` (`*` stands for any characters but `/`).
fn glob(pattern: &str, name: &str) -> bool {
    fn go(p: &[u8], n: &[u8]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some((b'*', rest)) => {
                let mut i = 0;
                loop {
                    if go(rest, &n[i..]) {
                        return true;
                    }
                    if i == n.len() || n[i] == b'/' {
                        return false;
                    }
                    i += 1;
                }
            }
            Some((c, rest)) => n.first() == Some(c) && go(rest, &n[1..]),
        }
    }
    go(pattern.as_bytes(), name.as_bytes())
}

/// Git's `--autosquash`: every `fixup!`/`squash!`/`amend!` commit moves behind the commit it
/// names (by subject, by commit id, by a subject's start) and becomes `fixup`, `squash` or
/// `fixup -C`.
fn autosquash(repo: &Repo, ids: &[ObjectId]) -> Vec<Line> {
    let subjects: Vec<String> = ids.iter().map(|id| repo.subject(*id)).collect();
    let n = ids.len();
    let mut next: Vec<Option<usize>> = vec![None; n];
    let mut tail: Vec<Option<usize>> = vec![None; n];
    let mut kinds: Vec<(GitRebaseStepKind, bool)> = vec![(GitRebaseStepKind::Pick, false); n];
    let mut moved = vec![false; n];
    let mut by_subject: HashMap<&str, usize> = HashMap::new();
    for i in 0..n {
        let subject = subjects[i].as_str();
        let mut target = None;
        if let Some(mut p) = skip_fixupish(subject) {
            loop {
                p = p.trim_start();
                match skip_fixupish(p) {
                    Some(rest) => p = rest,
                    None => break,
                }
            }
            target = by_subject.get(p).copied();
            if target.is_none()
                && !p.contains(' ')
                && let Some(id) = repo.try_resolve(p)
            {
                target = ids.iter().position(|c| *c == id).filter(|t| *t != i);
            }
            if target.is_none() {
                target = (0..i).find(|j| subjects[*j].starts_with(p));
            }
        }
        match target {
            Some(t) => {
                kinds[i] = if subject.starts_with("fixup!") {
                    (GitRebaseStepKind::Fixup, false)
                } else if subject.starts_with("amend!") {
                    (GitRebaseStepKind::Fixup, true)
                } else {
                    (GitRebaseStepKind::Squash, false)
                };
                let after = tail[t].unwrap_or(t);
                next[i] = next[after];
                next[after] = Some(i);
                tail[t] = Some(i);
                moved[i] = true;
            }
            None => {
                by_subject.entry(subject).or_insert(i);
            }
        }
    }
    let mut out = Vec::with_capacity(n);
    for (i, _) in moved.iter().enumerate().filter(|(_, m)| !**m) {
        let mut at = Some(i);
        while let Some(j) = at {
            out.push(Line {
                kind: kinds[j].0,
                commit: ids[j],
                message: None,
                take_message: kinds[j].1,
            });
            at = next[j];
        }
    }
    out
}

// ---- the state in Git's files ----------------------------------------------------------------

/// A rebase in progress as its files say.
struct OnDisk {
    head_name: Option<String>,
    onto: ObjectId,
    orig_head: ObjectId,
    done: Vec<TodoLine>,
    todo: Vec<TodoLine>,
    stopped: Option<ObjectId>,
    amend: Option<ObjectId>,
    fixups: Vec<(GitRebaseStepKind, ObjectId)>,
    message_squash: Option<String>,
    message_fixup: Option<String>,
    message: Option<String>,
    author: Option<gix::actor::Signature>,
}

/// `author-script` (`GIT_AUTHOR_NAME='…'`, as the sequencer writes it).
fn author_script(sig: &gix::actor::Signature) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let offset = sig.time.offset;
    let sign = if offset < 0 { '-' } else { '+' };
    let abs = offset.unsigned_abs();
    format!(
        "GIT_AUTHOR_NAME={}\nGIT_AUTHOR_EMAIL={}\nGIT_AUTHOR_DATE={}\n",
        q(&sig.name.to_str_lossy()),
        q(&sig.email.to_str_lossy()),
        q(&format!(
            "@{} {sign}{:02}{:02}",
            sig.time.seconds,
            abs / 3600,
            (abs % 3600) / 60
        ))
    )
}

fn parse_author_script(text: &str) -> Option<gix::actor::Signature> {
    let mut vals: HashMap<&str, String> = HashMap::new();
    for line in text.lines() {
        let (k, v) = line.split_once('=')?;
        let v = v.trim();
        let v = v
            .strip_prefix('\'')?
            .strip_suffix('\'')?
            .replace("'\\''", "'");
        vals.insert(k, v);
    }
    let date = vals.get("GIT_AUTHOR_DATE")?;
    let date = date.strip_prefix('@').unwrap_or(date);
    let (secs, tz) = date.split_once(' ').unwrap_or((date, "+0000"));
    let seconds: i64 = secs.parse().ok()?;
    let (sign, digits) = tz.split_at(1);
    let hh: i32 = digits.get(0..2)?.parse().ok()?;
    let mm: i32 = digits.get(2..4)?.parse().ok()?;
    let mut offset = hh * 3600 + mm * 60;
    if sign == "-" {
        offset = -offset;
    }
    Some(gix::actor::Signature {
        name: vals.get("GIT_AUTHOR_NAME")?.as_str().into(),
        email: vals.get("GIT_AUTHOR_EMAIL")?.as_str().into(),
        time: gix::date::Time { seconds, offset },
    })
}

fn todo_text(repo: &Repo, lines: &[Line]) -> String {
    let mut out = String::new();
    for l in lines {
        let subject = repo.subject(l.commit);
        if l.take_message {
            out.push_str(&format!("fixup -C {} {subject}\n", l.commit));
        } else {
            out.push_str(&format!("{} {} {subject}\n", word(l.kind), l.commit));
        }
    }
    out
}

fn to_line(l: &TodoLine) -> Option<Line> {
    match l {
        TodoLine::Step(s) => Some(Line {
            kind: s.kind,
            commit: ObjectId::from_hex(s.commit.as_bytes()).ok()?,
            message: s.message.clone(),
            take_message: false,
        }),
        TodoLine::FixupC(id) => Some(Line {
            kind: GitRebaseStepKind::Fixup,
            commit: *id,
            message: None,
            take_message: true,
        }),
        TodoLine::Other(_) => None,
    }
}

fn short_branch(full: &str) -> String {
    full.strip_prefix("refs/heads/").unwrap_or(full).to_owned()
}

impl Repo {
    fn rebase_dir(&self) -> PathBuf {
        self.git_dir().join(REBASE_MERGE)
    }

    fn messages_path(&self) -> PathBuf {
        self.namespace.data_dir(self.git_dir()).join(MESSAGES_FILE)
    }

    fn messages_name(&self) -> String {
        format!("{}/{MESSAGES_FILE}", self.namespace.name())
    }

    /// The files of a rebase in progress, by path relative to the Git folder (the journal
    /// keeps them; undo puts them back): `rebase-merge/*`, the files next to it, `MERGE_MSG`
    /// while a rebase runs, the product's messages file. Empty when none runs.
    pub(crate) fn rebase_files(&self) -> Result<Vec<(String, String)>> {
        let dir = self.rebase_dir();
        let mut out = Vec::new();
        let io = |e: std::io::Error| fail(format!("cannot read the rebase state: {e}"));
        if dir.is_dir() {
            let mut names: Vec<String> = Vec::new();
            for e in std::fs::read_dir(&dir).map_err(io)? {
                let e = e.map_err(io)?;
                if e.file_type().map_err(io)?.is_file() {
                    names.push(e.file_name().to_string_lossy().into_owned());
                }
            }
            names.sort();
            for n in names {
                let bytes = std::fs::read(dir.join(&n)).map_err(io)?;
                out.push((
                    format!("{REBASE_MERGE}/{n}"),
                    String::from_utf8_lossy(&bytes).into_owned(),
                ));
            }
            if let Ok(t) = std::fs::read_to_string(self.git_dir().join("MERGE_MSG")) {
                out.push(("MERGE_MSG".into(), t));
            }
        }
        for n in SIDE_FILES {
            if let Ok(t) = std::fs::read_to_string(self.git_dir().join(n)) {
                out.push((n.into(), t));
            }
        }
        if let Ok(t) = std::fs::read_to_string(self.messages_path()) {
            out.push((self.messages_name(), t));
        }
        Ok(out)
    }

    /// Puts the rebase files back from `after` (what is there now) to `before` (undo).
    pub(crate) fn restore_rebase_files(
        &self,
        before: &[(String, String)],
        after: &[(String, String)],
    ) -> Result<()> {
        let mine = self.messages_name();
        let allowed = |name: &str| {
            !name.contains("..")
                && (name.starts_with(&format!("{REBASE_MERGE}/"))
                    || name == "MERGE_MSG"
                    || SIDE_FILES.contains(&name)
                    || name == mine)
        };
        let io = |e: std::io::Error| fail(format!("cannot write the rebase state: {e}"));
        for (name, _) in after {
            if !allowed(name) {
                return Err(fail("invalid journal"));
            }
            match std::fs::remove_file(self.git_dir().join(name)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
        }
        if !before.iter().any(|(n, _)| n.starts_with(REBASE_MERGE)) {
            match std::fs::remove_dir_all(self.rebase_dir()) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
        }
        for (name, text) in before {
            if !allowed(name) {
                return Err(fail("invalid journal"));
            }
            let path = self.git_dir().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            std::fs::write(path, text).map_err(io)?;
        }
        Ok(())
    }

    /// Removes the rebase state (a finished rebase, or before a new stop is written).
    pub(crate) fn clear_rebase_files(&self) -> Result<()> {
        let now = self.rebase_files()?;
        self.restore_rebase_files(&[], &now)
    }

    fn read_disk(&self) -> Result<Option<OnDisk>> {
        let dir = self.rebase_dir();
        if !dir.is_dir() {
            return Ok(None);
        }
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
        let id = |f: &str| read(f).and_then(|t| ObjectId::from_hex(t.trim().as_bytes()).ok());
        let (Some(onto), Some(orig_head)) = (id("onto"), id("orig-head")) else {
            return Ok(None);
        };
        let head_name = read("head-name")
            .map(|h| h.trim().to_owned())
            .filter(|h| h.starts_with("refs/"));
        let mut done = parse_todo(&self.repo, &read("done").unwrap_or_default());
        let mut todo = parse_todo(&self.repo, &read("git-rebase-todo").unwrap_or_default());
        // The messages kept for these lines.
        if let Ok(text) = std::fs::read_to_string(self.messages_path())
            && let Ok(kept) = serde_json::from_str::<Messages>(&text)
            && ObjectId::from_hex(kept.orig_head.as_bytes()).ok() == Some(orig_head)
        {
            let map: BTreeMap<String, String> = kept.messages.into_iter().collect();
            for line in done.iter_mut().chain(todo.iter_mut()) {
                if let TodoLine::Step(s) = line
                    && let Some(m) = map.get(&line_key(s))
                {
                    s.message = Some(m.clone());
                }
            }
        }
        let fixups = read("current-fixups")
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (k, id) = l.split_once(' ')?;
                let kind = match k {
                    "fixup" => GitRebaseStepKind::Fixup,
                    "squash" => GitRebaseStepKind::Squash,
                    _ => return None,
                };
                Some((kind, ObjectId::from_hex(id.trim().as_bytes()).ok()?))
            })
            .collect();
        Ok(Some(OnDisk {
            head_name,
            onto,
            orig_head,
            done,
            todo,
            stopped: id("stopped-sha"),
            amend: id("amend"),
            fixups,
            message_squash: read("message-squash"),
            message_fixup: read("message-fixup"),
            message: read("message"),
            author: read("author-script").and_then(|t| parse_author_script(&t)),
        }))
    }

    /// Paths of unmerged index entries (repository-relative, leading `/`).
    fn unmerged(&self) -> Result<Vec<String>> {
        let index = self.load_index()?;
        let set: BTreeSet<String> = index
            .entries()
            .iter()
            .filter(|e| e.stage() != Stage::Unconflicted)
            .map(|e| format!("/{}", e.path(&index).to_str_lossy()))
            .collect();
        Ok(set.into_iter().collect())
    }

    fn message_of(&self, id: ObjectId) -> String {
        self.repo
            .find_commit(id)
            .ok()
            .and_then(|c| c.message_raw().ok().map(|m| m.to_str_lossy().into_owned()))
            .unwrap_or_default()
    }

    /// The rebase in progress (GitTailor's or one `git rebase` started), read from
    /// `.git/rebase-merge/`; `None` when there is none.
    pub fn rebase_state(&self) -> Result<Option<GitRebaseState>> {
        let whole = self.whole()?;
        let Some(d) = whole.read_disk()? else {
            return Ok(None);
        };
        let steps = |lines: &[TodoLine]| -> Vec<GitRebaseStep> {
            lines.iter().filter_map(to_line).map(|l| l.step()).collect()
        };
        let conflicts = whole.unmerged()?;
        let last_kind = d.done.last().and_then(to_line).map(|l| l.kind);
        let stop = if !conflicts.is_empty() {
            GitRebaseStop::Conflict
        } else if last_kind == Some(GitRebaseStepKind::Edit) {
            GitRebaseStop::Edit
        } else if last_kind == Some(GitRebaseStepKind::Reword) && d.amend.is_some() {
            GitRebaseStop::Message
        } else if last_kind.is_some() && d.message.is_some() {
            GitRebaseStop::Conflict
        } else {
            GitRebaseStop::Other
        };
        Ok(Some(GitRebaseState {
            branch: d.head_name.as_deref().map(short_branch),
            onto: d.onto.to_string(),
            orig_head: d.orig_head.to_string(),
            done: steps(&d.done),
            todo: steps(&d.todo),
            stopped_at: d.stopped.map(|s| s.to_string()),
            stop,
            conflicts,
        }))
    }

    /// The message the stopped step proposes: the combination of a `squash` chain, the message
    /// given for the step, that of the commit an `edit` or `reword` stopped at, the picked
    /// commit's after a conflict. `None` when no rebase is in progress.
    pub fn rebase_message(&self) -> Result<Option<String>> {
        let whole = self.whole()?;
        let Some(d) = whole.read_disk()? else {
            return Ok(None);
        };
        if let Some(s) = &d.message_squash
            && !d.fixups.is_empty()
        {
            if d.fixups.iter().all(|(k, _)| *k == GitRebaseStepKind::Fixup)
                && let Some(f) = &d.message_fixup
            {
                return Ok(Some(f.clone()));
            }
            return Ok(Some(clean_strip(s)));
        }
        if let Some(TodoLine::Step(s)) = d.done.last()
            && let Some(m) = given(&s.message)
        {
            return Ok(Some(m));
        }
        if let Some(amend) = d.amend {
            return Ok(Some(whole.message_of(amend)));
        }
        Ok(d.message
            .map(|m| clean_strip_as(&m, &self.comment_string())))
    }

    /// The comment string of commit messages: `core.commentString`, else `core.commentChar`,
    /// else `#` (also for `auto`, which picks a character only while Git edits a message).
    fn comment_string(&self) -> String {
        self.config_value("core.commentString")
            .or_else(|| self.config_value("core.commentChar"))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty() && v != "auto")
            .unwrap_or_else(|| "#".into())
    }
}

// ---- plans and runs ---------------------------------------------------------------------------

/// A rebase about to run: what moves, the lines, where they start.
struct Setup {
    /// Full name of the branch (`None`: a detached rebase).
    head_name: Option<String>,
    /// The branch's tip before.
    orig_head: ObjectId,
    onto: ObjectId,
    base: ObjectId,
    lines: Vec<Line>,
    /// Lines `..from` are done already (continue, skip).
    from: usize,
    start: ObjectId,
    chain: Option<Chain>,
    steps_done: Vec<GitRebasePlanStep>,
}

/// What a replay came to.
struct Outcome {
    setup: Setup,
    replayed: Replayed,
    halt: Option<Halt>,
}

impl Outcome {
    fn tip(&self) -> Option<ObjectId> {
        self.halt.is_none().then_some(self.replayed.current)
    }

    fn branch(&self) -> String {
        self.setup
            .head_name
            .as_deref()
            .map(short_branch)
            .unwrap_or_else(|| "HEAD".into())
    }

    fn preview(&self) -> GitRebasePreview {
        GitRebasePreview {
            branch: self.branch(),
            onto: self.setup.onto.to_string(),
            base: self.setup.base.to_string(),
            steps: self.replayed.steps.clone(),
            stops_at: self.halt.as_ref().map(Halt::at),
            tip: self.tip().map(|t| t.to_string()),
        }
    }
}

fn plan_steps(repo: &Repo, lines: &[Line]) -> Vec<GitRebasePlanStep> {
    lines
        .iter()
        .map(|l| GitRebasePlanStep {
            step: l.step(),
            subject: repo.subject(l.commit),
            result: None,
            conflicts: Vec::new(),
            empty: false,
        })
        .collect()
}

/// The committer of a plan (no identity needed to look) or of a run.
fn committer(
    repo: &Repo,
    identity: Option<&GitIdentity>,
    plan: bool,
) -> Result<gix::actor::Signature> {
    match repo.identity_or(identity) {
        Ok(id) => Ok(crate::commit::signature(&id)),
        Err(e) if !plan => Err(e),
        Err(_) => Ok(gix::actor::Signature {
            name: repo.product().name.into(),
            email: repo.product().email.into(),
            time: gix::date::Time::now_local_or_utc(),
        }),
    }
}

/// A request that cannot be set up: why, as a plan's block.
type Set<T> = std::result::Result<T, GitBlock>;

fn nope<T>(reason: GitBlockReason, message: impl Into<String>) -> Result<Set<T>> {
    Ok(Err(block(reason, message)))
}

impl Repo {
    /// The whole repository with objects written to memory (a plan's run).
    fn memory(&self) -> Result<Repo> {
        let mut whole = self.whole()?;
        whole.repo = whole.repo.with_object_memory();
        Ok(whole)
    }

    /// The commits after `base` up to `tip`, oldest first, without merge commits.
    fn replayed_commits(&self, tip: ObjectId, base: ObjectId) -> Result<Vec<ObjectId>> {
        let walk = self
            .repo
            .rev_walk([tip])
            .with_hidden([base])
            .all()
            .ctx("cannot walk the history")?;
        let mut out = Vec::new();
        for info in walk {
            let info = info.ctx("cannot walk the history")?;
            if info.parent_ids.len() <= 1 {
                out.push(info.id);
            }
        }
        out.reverse();
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn setup_start(
        &self,
        cx: &Ctx,
        branch: &Option<String>,
        onto: &str,
        upstream: &Option<String>,
        steps: &[GitRebaseStep],
        squash: bool,
    ) -> Result<Set<Setup>> {
        let name = match branch {
            Some(b) => b.clone(),
            None => match &cx.cur {
                Some(c) => c.clone(),
                None => {
                    return nope(
                        GitBlockReason::Detached,
                        "You are not on a branch. Switch to the branch you want to rebase first.",
                    );
                }
            },
        };
        let full = branch_full(&name);
        let Some(tip) = self.ref_commit(&full)? else {
            return nope(
                GitBlockReason::NotFound,
                format!("There is no branch \"{name}\"."),
            );
        };
        let Some(onto_id) = self.try_resolve(onto) else {
            return nope(
                GitBlockReason::NotFound,
                format!("There is no commit \"{onto}\"."),
            );
        };
        let base = match upstream {
            Some(u) => match self.try_resolve(u) {
                Some(id) => id,
                None => {
                    return nope(
                        GitBlockReason::NotFound,
                        format!("There is no commit \"{u}\"."),
                    );
                }
            },
            None => match self.repo.merge_base(tip, onto_id) {
                Ok(Some(id)) => id.detach(),
                Ok(None) | Err(_) => {
                    return nope(
                        GitBlockReason::Unsupported,
                        format!("{name} and {onto} have no history in common."),
                    );
                }
            },
        };
        let lines = if steps.is_empty() {
            let ids = self.replayed_commits(tip, base)?;
            if squash {
                autosquash(self, &ids)
            } else {
                ids.into_iter()
                    .map(|commit| Line {
                        kind: GitRebaseStepKind::Pick,
                        commit,
                        message: None,
                        take_message: false,
                    })
                    .collect()
            }
        } else {
            let mut lines = Vec::new();
            for s in steps {
                let Some(id) = self.try_resolve(&s.commit) else {
                    return nope(
                        GitBlockReason::NotFound,
                        format!("There is no commit \"{}\".", s.commit),
                    );
                };
                let parents = self
                    .repo
                    .find_commit(id)
                    .ctx("cannot read a commit")?
                    .parent_ids()
                    .count();
                if parents > 1 {
                    return nope(
                        GitBlockReason::Unsupported,
                        format!(
                            "{} is a merge commit; rebasing merges works on the command line for now.",
                            self.named(id)
                        ),
                    );
                }
                lines.push(Line {
                    kind: s.kind,
                    commit: id,
                    message: s.message.clone(),
                    take_message: false,
                });
            }
            lines
        };
        if let Some(first) = lines.iter().find(|l| l.kind != GitRebaseStepKind::Drop)
            && first.folds()
        {
            return nope(
                GitBlockReason::Unsupported,
                "The first commit cannot be squashed or fixed up: there is no commit before it to fold into.",
            );
        }
        Ok(Ok(Setup {
            head_name: Some(full),
            orig_head: tip,
            onto: onto_id,
            base,
            lines,
            from: 0,
            start: onto_id,
            chain: None,
            steps_done: Vec::new(),
        }))
    }

    /// The lines of a rebase in progress and where its replay goes on (`continue`, `skip`),
    /// with the stopped step concluded through `engine` (objects go where the engine writes).
    fn setup_resume(
        &self,
        cx: &Ctx,
        engine: &Engine<'_>,
        message: Option<&str>,
        skip: bool,
    ) -> Result<Set<Setup>> {
        let Some(d) = self.read_disk()? else {
            return nope(GitBlockReason::NothingToDo, "No rebase is in progress.");
        };
        if !skip && !self.unmerged()?.is_empty() {
            return nope(
                GitBlockReason::InProgress,
                "Some files still have conflicts: resolve them first, then continue.",
            );
        }
        if let Some(TodoLine::Other(o)) = d.todo.iter().find(|l| matches!(l, TodoLine::Other(_))) {
            return nope(
                GitBlockReason::Unsupported,
                format!(
                    "The rest of this rebase has a step that only Git runs (\"{o}\"). Continue it with git rebase --continue."
                ),
            );
        }
        let Some(head) = cx.head else {
            return nope(
                GitBlockReason::NothingToDo,
                "There is no commit to go on from.",
            );
        };
        let done: Vec<Line> = d.done.iter().filter_map(to_line).collect();
        let rest: Vec<Line> = d.todo.iter().filter_map(to_line).collect();
        let last = d.done.last().and_then(to_line);
        let from = done.len();
        let mut lines = done;
        lines.extend(rest);
        let mut steps = plan_steps(self, &lines);
        // The chain of a stopped squash or fixup.
        let mut chain = None;
        if !d.fixups.is_empty() {
            let h = engine.read(head)?;
            let first = match (&d.message_fixup, &d.message_squash) {
                (Some(f), _) => f.clone(),
                (None, Some(s)) => first_of_squash(s).unwrap_or_else(|| h.message.clone()),
                (None, None) => h.message.clone(),
            };
            let mut folded = Vec::new();
            let mut given_msg = None;
            for (kind, id) in &d.fixups {
                folded.push((*kind, *id, engine.read(*id)?.message));
                if let Some(l) = lines.iter().find(|l| l.commit == *id) {
                    if let Some(m) = given(&l.message) {
                        given_msg = Some(m);
                    }
                    if l.take_message {
                        given_msg = Some(engine.read(*id)?.message);
                    }
                }
            }
            chain = Some(Chain {
                head,
                parents: h.parents,
                author: h.author,
                first,
                folded,
                given: given_msg,
                owner: None,
            });
        }
        let mut current = head;
        let stopped_in_chain = last.as_ref().is_some_and(Line::folds) && chain.is_some();
        let at = from.checked_sub(1);
        if skip {
            if stopped_in_chain && let Some(c) = chain.as_mut() {
                c.folded.pop();
                if c.folded.is_empty() {
                    chain = None;
                }
            }
            if let Some(s) = at.and_then(|i| steps.get_mut(i)) {
                s.empty = true;
            }
        } else {
            let files: Vec<&GitFile> = cx
                .changes()?
                .iter()
                .filter(|f| f.state != GitFileState::Untracked)
                .collect();
            let tree = crate::undo::tree_with(self, self.commit_tree(head)?, &files)?;
            let given_now = message.map(clean_whitespace).filter(|m| !m.is_empty());
            let head_info = engine.read(head)?;
            if stopped_in_chain {
                let c = chain.as_mut().expect("in a chain");
                if let Some(m) = given_now {
                    c.given = Some(m);
                }
                let msg = c.message();
                current = engine.commit(tree, c.parents.clone(), c.author.clone(), &msg)?;
            } else if d.amend == Some(head) {
                // An `edit`, or a `reword` that waited for its message.
                if tree != head_info.tree || given_now.is_some() {
                    let msg = given_now.unwrap_or_else(|| head_info.message.clone());
                    current = engine.commit(
                        tree,
                        head_info.parents.clone(),
                        head_info.author.clone(),
                        &msg,
                    )?;
                }
                if let Some(s) = at.and_then(|i| steps.get_mut(i)) {
                    s.result = Some(current.to_string());
                }
            } else if let Some(l) = &last {
                // A pick that stopped on a conflict, resolved now.
                let stopped = d.stopped.unwrap_or(l.commit);
                let p = engine.read(stopped)?;
                if tree == head_info.tree && p.tree != p.parent_tree {
                    if let Some(s) = at.and_then(|i| steps.get_mut(i)) {
                        s.empty = true;
                    }
                } else {
                    let msg = given_now
                        .or_else(|| {
                            (l.kind == GitRebaseStepKind::Reword)
                                .then(|| given(&l.message))
                                .flatten()
                        })
                        // Newer Git writes the `# Conflicts:` block into the message and
                        // strips comments when it commits on continue; so does this.
                        .or_else(|| {
                            d.message
                                .as_deref()
                                .map(|m| clean_strip_as(m, &self.comment_string()))
                        })
                        .unwrap_or_else(|| p.message.clone());
                    let author = d.author.clone().unwrap_or(p.author.clone());
                    current = engine.commit(tree, vec![head], author, &msg)?;
                    if let Some(s) = at.and_then(|i| steps.get_mut(i)) {
                        s.result = Some(current.to_string());
                    }
                }
            }
        }
        if let Some(c) = chain.as_mut() {
            c.head = current;
        }
        steps.truncate(from);
        Ok(Ok(Setup {
            head_name: d.head_name,
            orig_head: d.orig_head,
            onto: d.onto,
            base: d.onto,
            lines,
            from,
            start: current,
            chain,
            steps_done: steps,
        }))
    }

    /// Replays `setup` with `engine`.
    fn replay_setup(&self, engine: &Engine<'_>, setup: Setup) -> Result<Outcome> {
        let mut steps = plan_steps(self, &setup.lines);
        for (i, s) in setup.steps_done.iter().enumerate() {
            steps[i] = s.clone();
        }
        let mut replayed = Replayed {
            current: setup.start,
            chain: setup.chain.clone(),
            steps,
        };
        let halt = engine.replay(&setup.lines, setup.from, &mut replayed)?;
        Ok(Outcome {
            setup,
            replayed,
            halt,
        })
    }

    /// The protected pattern `branch` matches, if any.
    fn protected_by(&self, branch: &str) -> Option<String> {
        self.protected.iter().find(|p| glob(p, branch)).cloned()
    }

    /// Fills `p` from a replay in memory.
    fn plan_from(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        outcome: &Outcome,
        allow_published: bool,
        what: &str,
    ) -> Result<GitPlan> {
        let s = &outcome.setup;
        let preview = outcome.preview();
        let branch = preview.branch.clone();
        let picks = s
            .lines
            .iter()
            .skip(s.from)
            .filter(|l| l.kind != GitRebaseStepKind::Drop)
            .count();
        let current = match (&cx.head_ref, &s.head_name) {
            (Some(h), Some(n)) => h.as_bstr() == n.as_str(),
            _ => false,
        };
        if s.from == 0 {
            p.steps.push(format!(
                "Replays {} of {branch} onto {}; {branch} moves once, at the end.",
                plural_s(picks, "commit"),
                self.named(s.onto)
            ));
        } else {
            p.steps.push(format!(
                "Goes on with the rebase of {branch}: {} still to replay.",
                plural_s(picks, "commit")
            ));
        }
        let dropped = s
            .lines
            .iter()
            .skip(s.from)
            .filter(|l| l.kind == GitRebaseStepKind::Drop)
            .count();
        if dropped > 0 {
            p.steps
                .push(format!("Leaves out {}.", plural_s(dropped, "commit")));
        }
        let empty = preview.steps.iter().filter(|st| st.empty).count();
        if empty > 0 {
            p.steps.push(format!(
                "{} {} on {} already and left out.",
                plural_s(empty, "change"),
                if empty == 1 { "is" } else { "are" },
                short(&s.onto)
            ));
        }
        match &outcome.halt {
            Some(Halt::Conflict { at, paths }) => p.steps.push(format!(
                "Stops at \"{}\": {} need{} your decision ({}).",
                self.subject(s.lines[*at].commit),
                plural_s(paths.len(), "file"),
                if paths.len() == 1 { "s" } else { "" },
                paths.join(", ")
            )),
            Some(Halt::Edit { at }) => p.steps.push(format!(
                "Stops after \"{}\" so you can change it.",
                self.subject(s.lines[*at].commit)
            )),
            Some(Halt::Message { at }) => p.steps.push(format!(
                "Stops at \"{}\" for its new message.",
                self.subject(s.lines[*at].commit)
            )),
            None => p
                .steps
                .push("The backup keeps the old commits for Undo.".into()),
        }
        // What the branch loses that is on a remote already.
        let mut hidden = vec![s.onto, s.base, outcome.replayed.current];
        hidden.sort();
        hidden.dedup();
        let published = self.published_in(s.orig_head, &hidden)?;
        if published > 0
            && let Some(full) = &s.head_name
        {
            let short_name = short_branch(full);
            if let Some(pattern) = self.protected_by(&short_name) {
                p.stop(Some(block(
                    GitBlockReason::Protected,
                    format!(
                        "{short_name} is protected ({pattern}): its commits on the server are not rewritten."
                    ),
                )));
            } else if !allow_published {
                p.stop(Some(block(
                    GitBlockReason::Published,
                    format!(
                        "{} you would replace {} on the server already. Confirm that you want to rewrite {}; a force push with lease follows.",
                        plural_s(published, "commit"),
                        if published == 1 { "is" } else { "are" },
                        if published == 1 { "it" } else { "them" }
                    ),
                )));
            } else {
                p.warnings.push(format!(
                    "{} on the server will be replaced: push with lease afterwards.",
                    plural_s(published, "commit")
                ));
            }
        }
        let tip = outcome.tip();
        if let Some(full) = &s.head_name {
            p.refs.push(GitRefMove {
                name: full.clone(),
                before: self.ref_value(full)?.map(|v| v.to_string()),
                after: Some(tip.map(|t| t.to_string()).unwrap_or_default()),
            });
        }
        if s.from == 0 && tip == Some(s.orig_head) {
            p.undoable = false;
            p.stop(Some(block(
                GitBlockReason::NothingToDo,
                format!("{branch} would not change."),
            )));
        }
        let touches = current || outcome.halt.is_some() || s.from > 0;
        if touches {
            let to = tip.unwrap_or(outcome.replayed.current);
            p.files = diff_paths(&self.flat_of(cx.head)?, &self.flat_of(Some(to))?);
        }
        p.backup = true;
        p.rebase = Some(preview);
        p.stop(cx.unsaved());
        if s.from == 0 && touches {
            p.stop(cx.local_changes(what)?);
        }
        Ok(p)
    }

    fn in_rebase(&self) -> Option<GitBlock> {
        self.rebase_dir().is_dir().then(|| {
            block(
                GitBlockReason::InProgress,
                "A rebase is in progress: continue, skip or abort it first.",
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn plan_rebase(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        branch: &Option<String>,
        onto: &str,
        upstream: &Option<String>,
        steps: &[GitRebaseStep],
        autosquash: bool,
        allow_published: bool,
    ) -> Result<GitPlan> {
        p.stop(self.in_rebase());
        p.stop(cx.in_progress()?);
        if p.blocked.is_some() {
            return Ok(p);
        }
        let m = self.memory()?;
        let setup = match m.setup_start(cx, branch, onto, upstream, steps, autosquash)? {
            Ok(s) => s,
            Err(b) => return Ok(p.blocked_by(b)),
        };
        let engine = Engine {
            repo: &m,
            committer: committer(&m, None, true)?,
            sign: false,
        };
        let outcome = m.replay_setup(&engine, setup)?;
        m.plan_from(cx, p, &outcome, allow_published, "rebase")
    }

    pub(crate) fn plan_rebase_resume(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        message: Option<&str>,
        skip: bool,
    ) -> Result<GitPlan> {
        let m = self.memory()?;
        let engine = Engine {
            repo: &m,
            committer: committer(&m, None, true)?,
            sign: false,
        };
        let setup = match m.setup_resume(cx, &engine, message, skip)? {
            Ok(s) => s,
            Err(b) => {
                if b.reason == GitBlockReason::NothingToDo {
                    p.undoable = false;
                }
                return Ok(p.blocked_by(b));
            }
        };
        let outcome = m.replay_setup(&engine, setup)?;
        if skip {
            p.destructive = !cx.tracked_changes()?.is_empty();
        }
        m.plan_from(cx, p, &outcome, true, "continue")
    }

    pub(crate) fn plan_pull_rebase(&self, cx: &Ctx, mut p: GitPlan) -> Result<GitPlan> {
        p.stop(self.in_rebase());
        p.stop(cx.need_branch());
        p.network = true;
        p.backup = true;
        let Some(cur) = cx.cur.clone() else {
            return Ok(p);
        };
        let Some(up) = self.upstream()? else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NoUpstream,
                format!("{cur} is not on the server yet. Publish it first."),
            )));
        };
        p.steps.push(format!(
            "Gets new commits from {} and replays the commits of {cur} on top of them.",
            up.display
        ));
        p.refs.push(GitRefMove {
            name: branch_full(&cur),
            before: cx.head.map(|h| h.to_string()),
            after: Some(String::new()),
        });
        // From what was fetched last (the run fetches first).
        if let (Some(head), Some(theirs)) = (cx.head, self.ref_commit(&up.tracking)?)
            && !self.is_ancestor(theirs, head)
        {
            let m = self.memory()?;
            if let Ok(setup) = m.setup_start(cx, &None, &up.tracking, &None, &[], false)? {
                let engine = Engine {
                    repo: &m,
                    committer: committer(&m, None, true)?,
                    sign: false,
                };
                let outcome = m.replay_setup(&engine, setup)?;
                p.rebase = Some(outcome.preview());
            }
        }
        p.stop(cx.unsaved());
        p.stop(cx.in_progress()?);
        p.stop(cx.local_changes("get changes with a rebase")?);
        Ok(p)
    }

    /// Ends a replay on the real repository: the branch moves to the tip (and the files with
    /// it), or the rebase stops in Git's format.
    fn conclude(&self, cx: &Ctx, outcome: Outcome, what: &str) -> Result<Done> {
        let s = &outcome.setup;
        let reflog = self.reflog_message(what);
        let current_branch = match (&cx.head_ref, &s.head_name) {
            (Some(h), Some(n)) => h.as_bstr() == n.as_str(),
            _ => false,
        };
        let whole = &cx.whole;
        let Some(halt) = &outcome.halt else {
            let tip = outcome.replayed.current;
            // Finished: the branch moves once; its files follow when it is checked out.
            if s.from == 0 && !current_branch {
                if let Some(full) = &s.head_name {
                    self.put_ref(full, Some(tip), Some(s.orig_head), &reflog)?;
                }
                return Ok(Done {
                    message: finished(self, s, tip),
                    outcome: Some(GitPullResult::Merged {
                        oid: tip.to_string(),
                    }),
                    reload: false,
                });
            }
            let now = crate::undo::snapshot_tree(whole)?;
            let want = self.keep_untracked_files(cx, now, self.commit_tree(tip)?)?;
            match &s.head_name {
                Some(full) => {
                    let before = self.ref_value(full)?;
                    self.put_ref(full, Some(tip), before, &reflog)?;
                    let name = gix::refs::FullName::try_from(full.as_str())
                        .map_err(|_| fail("invalid branch name in the rebase"))?;
                    self.set_head(&name, &reflog)?;
                }
                None => self.detach_head(tip, &reflog)?,
            }
            self.clear_rebase_files()?;
            self.clear_merge_state()?;
            self.reopen()?.whole()?.move_worktree(now, want, what)?;
            return Ok(Done {
                message: finished(self, s, tip),
                outcome: Some(GitPullResult::Merged {
                    oid: tip.to_string(),
                }),
                reload: true,
            });
        };
        // Stopped: HEAD at the last commit made, the files of that commit, then the stop.
        let line = &s.lines[halt.at()];
        let current = outcome
            .replayed
            .chain
            .as_ref()
            .map_or(outcome.replayed.current, |c| c.head);
        let now = crate::undo::snapshot_tree(whole)?;
        let want = self.keep_untracked_files(cx, now, self.commit_tree(current)?)?;
        self.detach_head(current, &reflog)?;
        let repo = self.reopen()?;
        let whole = repo.whole()?;
        whole.move_worktree(now, want, what)?;
        let picked = Engine {
            repo: &whole,
            committer: committer(&whole, None, true)?,
            sign: false,
        }
        .read(line.commit)?;
        let mut conflicts = Vec::new();
        if let Halt::Conflict { .. } = halt {
            let label = label(&whole, line.commit);
            let tw = ThreeWay {
                kind: MergeKind::CherryPick,
                base: picked.parent,
                ours: current,
                theirs: line.commit,
                label: &label,
            };
            let computed = whole.compute_merge_with(&whole.repo, &tw, true)?;
            let (_, mut index, _) = whole.stage_merge(computed, &tw, what)?;
            whole.write_index(&mut index)?;
            conflicts = whole.unmerged()?;
        }
        whole.write_stop(&outcome, current, &picked, &conflicts)?;
        let subject = self.subject(line.commit);
        let message = match halt {
            Halt::Conflict { .. } => format!(
                "The rebase stopped at \"{subject}\": {} need{} your decision. Resolve, then continue.",
                plural_s(conflicts.len(), "file"),
                if conflicts.len() == 1 { "s" } else { "" }
            ),
            Halt::Edit { .. } => {
                format!("The rebase stopped after \"{subject}\": change it, then continue.")
            }
            Halt::Message { .. } => format!(
                "The rebase stopped at \"{subject}\": give it its new message, then continue."
            ),
        };
        Ok(Done {
            message,
            outcome: matches!(halt, Halt::Conflict { .. })
                .then_some(GitPullResult::Conflicts { paths: conflicts }),
            reload: true,
        })
    }

    /// `to_tree` with the untracked files of `now` it does not have.
    fn keep_untracked_files(&self, cx: &Ctx, now: ObjectId, to_tree: ObjectId) -> Result<ObjectId> {
        let now_flat = flatten(&self.repo, now)?;
        let to_flat = flatten(&self.repo, to_tree)?;
        let mut editor = self.repo.edit_tree(to_tree).ctx("cannot build the tree")?;
        for f in cx.changes()? {
            let path = BString::from(f.path.as_str());
            if f.state == GitFileState::Untracked
                && !to_flat.contains_key(&path)
                && let Some(blob) = now_flat.get(&path)
            {
                editor
                    .upsert(
                        path.as_bstr(),
                        crate::commit::entry_kind(blob.mode),
                        blob.id,
                    )
                    .ctx("cannot build the tree")?;
            }
        }
        Ok(editor.write().ctx("cannot write the tree")?.detach())
    }

    /// Writes the state of a stopped rebase in Git's format.
    fn write_stop(
        &self,
        outcome: &Outcome,
        current: ObjectId,
        picked: &Picked,
        conflicts: &[String],
    ) -> Result<()> {
        let s = &outcome.setup;
        let halt = outcome.halt.as_ref().expect("stopped");
        let at = halt.at();
        let line = &s.lines[at];
        self.clear_rebase_files()?;
        let dir = self.rebase_dir();
        let io = |e: std::io::Error| fail(format!("cannot write the rebase state: {e}"));
        std::fs::create_dir_all(&dir).map_err(io)?;
        let put = |name: &str, text: &str| std::fs::write(dir.join(name), text).map_err(io);
        put(
            "head-name",
            &format!("{}\n", s.head_name.as_deref().unwrap_or("detached HEAD")),
        )?;
        put("onto", &format!("{}\n", s.onto))?;
        put("orig-head", &format!("{}\n", s.orig_head))?;
        put("interactive", "")?;
        put("drop_redundant_commits", "")?;
        put("no-reschedule-failed-exec", "")?;
        put("done", &todo_text(self, &s.lines[..=at]))?;
        put("git-rebase-todo", &todo_text(self, &s.lines[at + 1..]))?;
        put("msgnum", &format!("{}\n", at + 1))?;
        put("end", &format!("{}\n", s.lines.len()))?;
        put("stopped-sha", &format!("{}\n", line.commit))?;
        put("author-script", &author_script(&picked.author))?;
        let chain = outcome.replayed.chain.as_ref().filter(|_| line.folds());
        let message = match (halt, chain) {
            (_, Some(c)) => c.squash_text(),
            (Halt::Conflict { .. }, None) => {
                if line.kind == GitRebaseStepKind::Reword {
                    given(&line.message).unwrap_or_else(|| picked.message.clone())
                } else {
                    picked.message.clone()
                }
            }
            _ => self.message_of(current),
        };
        put("message", &message)?;
        if !matches!(halt, Halt::Conflict { .. }) || chain.is_some() {
            put("amend", &format!("{current}\n"))?;
        }
        if let Some(c) = chain {
            put("current-fixups", &c.fixups_text())?;
            put("message-squash", &c.squash_text())?;
            if c.only_fixups() {
                put("message-fixup", &c.first)?;
            } else {
                std::fs::write(self.git_dir().join("SQUASH_MSG"), c.squash_text()).map_err(io)?;
            }
        }
        std::fs::write(
            self.git_dir().join("REBASE_HEAD"),
            format!("{}\n", line.commit),
        )
        .map_err(io)?;
        if !conflicts.is_empty() {
            let mut m = message.clone();
            if !m.ends_with('\n') {
                m.push('\n');
            }
            m.push_str("\n# Conflicts:\n");
            for c in conflicts {
                m.push_str(&format!("#\t{}\n", c.trim_start_matches('/')));
            }
            std::fs::write(self.git_dir().join("MERGE_MSG"), m).map_err(io)?;
        }
        // The messages of the lines (Git's todo has no place for them).
        let kept: Vec<(String, String)> = s
            .lines
            .iter()
            .filter_map(|l| l.message.clone().map(|m| (line_key(&l.step()), m)))
            .collect();
        if !kept.is_empty() {
            let path = self.messages_path();
            std::fs::create_dir_all(path.parent().expect("has a parent")).map_err(io)?;
            let text = serde_json::to_string(&Messages {
                orig_head: s.orig_head.to_string(),
                messages: kept,
            })
            .ctx("cannot write the rebase state")?;
            std::fs::write(path, text).map_err(io)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_rebase(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        branch: &Option<String>,
        onto: &str,
        upstream: &Option<String>,
        steps: &[GitRebaseStep],
        autosquash: bool,
    ) -> Result<Done> {
        let setup = self
            .setup_start(cx, branch, onto, upstream, steps, autosquash)?
            .map_err(|b| fail(b.message))?;
        let name = setup
            .head_name
            .as_deref()
            .map(short_branch)
            .unwrap_or_else(|| "HEAD".into());
        run.backup(self, &name, Some(setup.orig_head))?;
        let engine = Engine {
            repo: self,
            committer: committer(self, run.opts.identity, false)?,
            sign: true,
        };
        let outcome = self.replay_setup(&engine, setup)?;
        self.conclude(cx, outcome, "rebase")
    }

    pub(crate) fn run_rebase_resume(
        &self,
        cx: &Ctx,
        run: &mut Run<'_>,
        message: Option<&str>,
        skip: bool,
    ) -> Result<Done> {
        let engine = Engine {
            repo: self,
            committer: committer(self, run.opts.identity, false)?,
            sign: true,
        };
        if let Some(full) = self.read_disk()?.and_then(|d| d.head_name) {
            run.backup(self, &short_branch(&full), self.ref_value(&full)?)?;
        }
        if let Some(h) = cx.head {
            run.backup(self, "HEAD", Some(h))?;
        }
        let setup = self
            .setup_resume(cx, &engine, message, skip)?
            .map_err(|b| fail(b.message))?;
        let outcome = self.replay_setup(&engine, setup)?;
        self.conclude(
            cx,
            outcome,
            if skip {
                "rebase (skip)"
            } else {
                "rebase (continue)"
            },
        )
    }

    pub(crate) fn run_pull_rebase(&self, cx: &Ctx, run: &mut Run<'_>) -> Result<Done> {
        let up = self
            .upstream()?
            .ok_or_else(|| fail("This branch has no upstream yet. Publish it first."))?;
        let cur = cx
            .cur
            .clone()
            .ok_or_else(|| fail("You are not on a branch."))?;
        let head = cx
            .head
            .ok_or_else(|| fail("Make the first commit first."))?;
        run.backup(self, &cur, Some(head))?;
        self.fetch(&up.remote, run.opts.credentials, false)?;
        let repo = self.reopen()?;
        let theirs = repo
            .ref_commit(&up.tracking)?
            .ok_or_else(|| fail(format!("{} has no branch {} yet.", up.remote, up.display)))?;
        if repo.is_ancestor(theirs, head) {
            return Ok(Done {
                message: "Already up to date.".into(),
                outcome: Some(GitPullResult::UpToDate),
                reload: false,
            });
        }
        let cx = repo.ctx(cx.unsaved)?;
        let forward = repo.is_ancestor(head, theirs);
        let commits = u32::try_from(repo.count_hidden(theirs, &[head])?).unwrap_or(u32::MAX);
        let setup = repo
            .setup_start(&cx, &None, &up.tracking, &None, &[], false)?
            .map_err(|b| fail(b.message))?;
        let engine = Engine {
            repo: &repo,
            committer: committer(&repo, run.opts.identity, false)?,
            sign: true,
        };
        let outcome = repo.replay_setup(&engine, setup)?;
        let mut done = repo.conclude(&cx, outcome, "pull --rebase")?;
        if forward {
            done.message = format!(
                "Got {} from {}.",
                plural_s(commits as usize, "commit"),
                up.display
            );
            done.outcome = Some(GitPullResult::FastForward { commits });
        }
        Ok(done)
    }
}

/// `"two\n"` from a `message-squash`: the first commit's message.
fn first_of_squash(text: &str) -> Option<String> {
    let head = format!("# {}\n\n", ordinal(1));
    let start = text.find(&head)? + head.len();
    let rest = &text[start..];
    let end = rest
        .find("\n# This is the commit message #")
        .or_else(|| rest.find("\n# The commit message #"))
        .unwrap_or(rest.len());
    Some(format!("{}\n", rest[..end].trim_end_matches('\n')))
}

fn finished(repo: &Repo, s: &Setup, tip: ObjectId) -> String {
    let branch = s
        .head_name
        .as_deref()
        .map(short_branch)
        .unwrap_or_else(|| "HEAD".into());
    format!(
        "Rebased {branch} onto {}; it now points at {}.",
        short(&s.onto),
        repo.named(tip)
    )
}

/// A rebase replayed for the dry run (`dry_run.rs`).
pub(crate) struct DryRebase {
    pub head_name: Option<String>,
    pub orig_head: ObjectId,
    /// The new tip; `None` when a step stops.
    pub tip: Option<ObjectId>,
    pub conflicts: Vec<String>,
    pub message: String,
}

impl Repo {
    /// The rebase of `action` replayed with objects written to `self` (the dry run's memory),
    /// signed and by the committer the run has, so accepting it writes exactly these commits.
    pub(crate) fn dry_rebase(
        &self,
        cx: &Ctx,
        action: &GitAction,
        identity: Option<&GitIdentity>,
    ) -> Result<DryRebase> {
        let GitAction::Rebase {
            branch,
            onto,
            upstream,
            steps,
            autosquash,
            ..
        } = action
        else {
            return Err(fail("not a rebase"));
        };
        let setup = self
            .setup_start(cx, branch, onto, upstream, steps, *autosquash)?
            .map_err(|b| fail(b.message))?;
        let engine = Engine {
            repo: self,
            committer: committer(self, identity, false)?,
            sign: true,
        };
        let outcome = self.replay_setup(&engine, setup)?;
        let tip = outcome.tip();
        let conflicts = match &outcome.halt {
            Some(Halt::Conflict { paths, .. }) => paths.clone(),
            _ => Vec::new(),
        };
        let message = match tip {
            Some(t) => finished(self, &outcome.setup, t),
            None => {
                "The rebase would stop before its end; accepting runs it and stops there.".into()
            }
        };
        Ok(DryRebase {
            head_name: outcome.setup.head_name.clone(),
            orig_head: outcome.setup.orig_head,
            tip,
            conflicts,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanups_follow_git() {
        assert_eq!(clean_whitespace("\n\n a  \n\n\n b\n\n"), " a\n\n b\n");
        assert_eq!(clean_strip("# x\none\n# y\n\ntwo"), "one\n\ntwo\n");
        assert_eq!(
            clean_strip_as("feat notes\n\n; Conflicts:\n;\tnotes.txt\n", ";"),
            "feat notes\n"
        );
        assert_eq!(clean_whitespace("   \n"), "");
    }

    #[test]
    fn globs_stop_at_slashes() {
        assert!(glob("release/*", "release/1.0"));
        assert!(!glob("release/*", "release/1/2"));
        assert!(glob("main", "main"));
        assert!(!glob("main", "mainline"));
        assert!(glob("*", "feature"));
        assert!(!glob("*", "a/b"));
    }

    #[test]
    fn author_script_round_trips() {
        let sig = gix::actor::Signature {
            name: "O'Brien".into(),
            email: "o@x".into(),
            time: gix::date::Time {
                seconds: 1_700_000_000,
                offset: -5400,
            },
        };
        let text = author_script(&sig);
        assert!(text.contains("GIT_AUTHOR_DATE='@1700000000 -0130'"));
        assert_eq!(parse_author_script(&text), Some(sig));
    }

    #[test]
    fn first_message_of_a_squash_file() {
        let t = "# This is a combination of 2 commits.\n# This is the 1st commit message:\n\ntwo\n\n# This is the commit message #2:\n\n# fixup! one\n";
        assert_eq!(first_of_squash(t).as_deref(), Some("two\n"));
    }
}

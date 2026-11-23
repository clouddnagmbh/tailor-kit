// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Wave 15, strand C: `search_history` finds commits by message, author, path and content, in
//! the graph's order, in pages. Compared with `git log` (`--grep`, `--author`, `-S`, a path)
//! when Git is installed.

mod common;

use std::path::Path;
use std::process::Command;

use common::*;
use gix::ObjectId;
use gix::objs::tree::EntryKind;
use gix::refs::transaction::PreviousValue;
use rt_app::git::history::{GitHistoryField, GitHistoryPage, GitHistoryQuery};

fn snapshot(
    g: &gix::Repository,
    parents: &[ObjectId],
    files: &[(&str, &[u8])],
    message: &str,
    time: i64,
    who: &str,
) -> ObjectId {
    let mut editor = g.edit_tree(ObjectId::empty_tree(g.object_hash())).unwrap();
    for (path, bytes) in files {
        let id = g.write_blob(bytes).unwrap().detach();
        editor.upsert(*path, EntryKind::Blob, id).unwrap();
    }
    let tree = editor.write().unwrap().detach();
    let sig = gix::actor::Signature {
        name: who.into(),
        email: format!("{}@example.com", who.to_lowercase()).into(),
        time: gix::date::Time::new(time, 0),
    };
    let mut buf = gix::date::parse::TimeBuf::default();
    let sig = sig.to_ref(&mut buf);
    g.new_commit_as(sig, sig, message, tree, parents.iter().copied())
        .unwrap()
        .id
}

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn git_log(dir: &Path, args: &[&str]) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["log", "--all", "--date-order", "--format=%H"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

struct Fixture {
    _t: Tmp,
    dir: std::path::PathBuf,
    ids: Vec<(&'static str, ObjectId)>,
}

impl Fixture {
    fn id(&self, name: &str) -> String {
        self.ids
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1
            .to_string()
    }
    fn ids(&self, names: &[&str]) -> Vec<String> {
        names.iter().map(|n| self.id(n)).collect()
    }
}

fn oids(page: &GitHistoryPage) -> Vec<String> {
    page.hits
        .iter()
        .map(|h| h.commit.commit.oid.clone())
        .collect()
}

fn query() -> GitHistoryQuery {
    GitHistoryQuery {
        limit: 100,
        ..Default::default()
    }
}

/// ```text
///  c1 Ada    "Add users endpoint"   api/users.yml, README.md
///  c2 Grace  "Fix pagination"       api/users.yml edited
///  c3 Linus  "Add logo"             logo.png
///  f1 Ada    "Document tokens"      README.md gets a secret_token line   (branch feature, from c2)
///  c4 Grace  "Rename users"         api/users.yml -> api/people.yml
///  merge Linus "Merge feature"
///  c5 Ada    "Remove token docs"    README.md loses the line
/// ```
fn fixture() -> Fixture {
    let t = tmp("history-search");
    let dir = t.path().to_path_buf();
    rt_git::init(&dir, &opts()).unwrap();
    set_user(&dir, "Ada");
    let g = gix::open_opts(&dir, gix::open::Options::isolated()).unwrap();
    let readme: &[u8] = b"# Billing\n";
    let readme_tok: &[u8] = b"# Billing\nUse the SECRET_TOKEN header\n";
    let c1 = snapshot(
        &g,
        &[],
        &[("api/users.yml", b"url: /users\n"), ("README.md", readme)],
        "Add users endpoint",
        1_000,
        "Ada",
    );
    let c2 = snapshot(
        &g,
        &[c1],
        &[
            ("api/users.yml", b"url: /users?page=1\n"),
            ("README.md", readme),
        ],
        "Fix pagination",
        2_000,
        "Grace",
    );
    let c3 = snapshot(
        &g,
        &[c2],
        &[
            ("api/users.yml", b"url: /users?page=1\n"),
            ("README.md", readme),
            ("logo.png", b"\x89PNG\0\0"),
        ],
        "Add logo",
        3_000,
        "Linus",
    );
    let f1 = snapshot(
        &g,
        &[c2],
        &[
            ("api/users.yml", b"url: /users?page=1\n"),
            ("README.md", readme_tok),
        ],
        "Document tokens",
        3_500,
        "Ada",
    );
    let c4 = snapshot(
        &g,
        &[c3],
        &[
            ("api/people.yml", b"url: /users?page=1\n"),
            ("README.md", readme),
            ("logo.png", b"\x89PNG\0\0"),
        ],
        "Rename users",
        4_000,
        "Grace",
    );
    let merge = snapshot(
        &g,
        &[c4, f1],
        &[
            ("api/people.yml", b"url: /users?page=1\n"),
            ("README.md", readme_tok),
            ("logo.png", b"\x89PNG\0\0"),
        ],
        "Merge feature",
        5_000,
        "Linus",
    );
    let c5 = snapshot(
        &g,
        &[merge],
        &[
            ("api/people.yml", b"url: /users?page=1\n"),
            ("README.md", readme),
            ("logo.png", b"\x89PNG\0\0"),
        ],
        "Remove token docs",
        6_000,
        "Ada",
    );
    g.reference("refs/heads/main", c5, PreviousValue::Any, "t")
        .unwrap();
    g.reference("refs/heads/feature", f1, PreviousValue::Any, "t")
        .unwrap();
    Fixture {
        _t: t,
        dir,
        ids: vec![
            ("c1", c1),
            ("c2", c2),
            ("c3", c3),
            ("f1", f1),
            ("c4", c4),
            ("merge", merge),
            ("c5", c5),
        ],
    }
}

#[test]
fn finds_by_message_and_author_case_insensitively() {
    let f = fixture();
    let repo = reopen(&f.dir);
    let page = repo
        .search_history(&GitHistoryQuery {
            message: Some("FIX".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c2"]));
    assert_eq!(page.hits[0].matched, vec![GitHistoryField::Message]);
    assert!(!page.more);

    let page = repo
        .search_history(&GitHistoryQuery {
            author: Some("grace".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c4", "c2"]));
    // The email counts too.
    let page = repo
        .search_history(&GitHistoryQuery {
            author: Some("ada@example".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c5", "f1", "c1"]));

    // Every field has to match.
    let page = repo
        .search_history(&GitHistoryQuery {
            message: Some("a".into()),
            author: Some("linus".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["merge", "c3"]));
    assert_eq!(
        page.hits[0].matched,
        vec![GitHistoryField::Message, GitHistoryField::Author]
    );
    if have_git() {
        assert_eq!(
            oids(
                &repo
                    .search_history(&GitHistoryQuery {
                        author: Some("grace".into()),
                        ..query()
                    })
                    .unwrap()
            ),
            git_log(&f.dir, &["--author=grace"])
        );
        assert_eq!(
            oids(
                &repo
                    .search_history(&GitHistoryQuery {
                        message: Some("a".into()),
                        author: Some("linus".into()),
                        ..query()
                    })
                    .unwrap()
            ),
            git_log(&f.dir, &["--author=linus", "--grep=a", "-i"])
        );
    }
}

#[test]
fn finds_by_path_and_content_like_git_log() {
    let f = fixture();
    let repo = reopen(&f.dir);
    // Folders match everything below; renames count for both paths; merges are not listed.
    let page = repo
        .search_history(&GitHistoryQuery {
            path: Some("/api".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c4", "c2", "c1"]));
    assert_eq!(
        page.hits[0].paths,
        vec!["/api/people.yml", "/api/users.yml"]
    );
    assert_eq!(page.hits[0].matched, vec![GitHistoryField::Path]);
    let page = repo
        .search_history(&GitHistoryQuery {
            path: Some("/logo.png".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c3"]));
    let page = repo
        .search_history(&GitHistoryQuery {
            path: Some("/api/people".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c4"]));

    // Content: added or removed lines with the text, in any case.
    let page = repo
        .search_history(&GitHistoryQuery {
            content: Some("secret_token".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c5", "f1"]));
    assert_eq!(page.hits[0].paths, vec!["/README.md"]);
    assert_eq!(page.hits[0].matched, vec![GitHistoryField::Content]);
    let page = repo
        .search_history(&GitHistoryQuery {
            content: Some("page=1".into()),
            path: Some("/api".into()),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["c2"]));
    assert_eq!(
        page.hits[0].matched,
        vec![GitHistoryField::Path, GitHistoryField::Content]
    );
    // The rename moved the line "url: /users?page=1" to another file: not an added or removed
    // line (as `git log -S`), but a search by path finds the commit.
    let page = repo
        .search_history(&GitHistoryQuery {
            content: Some("nothing here".into()),
            ..query()
        })
        .unwrap();
    assert!(page.hits.is_empty());

    if have_git() {
        let git =
            |args: &[&str]| git_log(&f.dir, &[&["--no-merges", "--full-history"], args].concat());
        assert_eq!(
            oids(
                &repo
                    .search_history(&GitHistoryQuery {
                        path: Some("/api".into()),
                        ..query()
                    })
                    .unwrap()
            ),
            git(&["--", "api"])
        );
        assert_eq!(
            oids(
                &repo
                    .search_history(&GitHistoryQuery {
                        content: Some("secret_token".into()),
                        ..query()
                    })
                    .unwrap()
            ),
            git(&["-i", "-S", "SECRET_TOKEN"])
                .into_iter()
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn pages_are_stable_and_cover_the_history_once() {
    let f = fixture();
    let repo = reopen(&f.dir);
    let all = repo.search_history(&query()).unwrap();
    assert_eq!(
        oids(&all),
        f.ids(&["c5", "merge", "c4", "f1", "c3", "c2", "c1"])
    );
    assert!(!all.more);
    let mut joined = Vec::new();
    let mut skip = 0;
    loop {
        let page = repo
            .search_history(&GitHistoryQuery {
                skip,
                limit: 3,
                ..query()
            })
            .unwrap();
        assert!(page.hits.len() <= 3);
        skip += page.hits.len() as u32;
        joined.extend(oids(&page));
        if !page.more {
            break;
        }
    }
    assert_eq!(joined, oids(&all));
    // The hits carry the graph's rows: parents and ref labels.
    let tip = &all.hits[0].commit;
    assert_eq!(tip.refs, vec!["refs/heads/main"]);
    assert_eq!(tip.parents, vec![f.id("merge")]);
    if have_git() {
        assert_eq!(oids(&all), git_log(&f.dir, &[]));
    }
}

#[test]
fn time_range_and_ref_restrictions() {
    let f = fixture();
    let repo = reopen(&f.dir);
    let page = repo
        .search_history(&GitHistoryQuery {
            since: Some(2_000),
            until: Some(3_500),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["f1", "c3", "c2"]));
    let page = repo
        .search_history(&GitHistoryQuery {
            refs: Some(vec!["refs/heads/feature".into()]),
            ..query()
        })
        .unwrap();
    assert_eq!(oids(&page), f.ids(&["f1", "c2", "c1"]));
    assert!(
        repo.search_history(&GitHistoryQuery {
            refs: Some(vec!["nope".into()]),
            ..query()
        })
        .is_err()
    );
}

#[test]
fn an_empty_repository_has_no_hits() {
    let t = tmp("history-search-empty");
    rt_git::init(t.path(), &opts()).unwrap();
    let repo = reopen(t.path());
    let page = repo.search_history(&query()).unwrap();
    assert!(page.hits.is_empty() && !page.more);
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! The prompt loader: prompts kept in versioned files, supplied by the product as
//! `(id, source)` pairs (`include_str!` of its own `prompts/<id>.md`).
//!
//! A file starts with a header between `---` lines:
//!
//! ```text
//! ---
//! id: suggest-names
//! version: 1
//! answer: json        # json | markdown | none (a fragment)
//! profile: fast       # fast | smart | none (a fragment)
//! ---
//! the text, with <<name>> for a value of the task and <<include:id>> for a fragment
//! ```
//!
//! A change of a prompt raises `version`; the product keeps recorded answers per version.
//! Values of the task are inserted in one pass, so a description that contains `<<context>>`
//! stays text.

/// One parsed prompt file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prompt {
    pub id: &'static str,
    pub version: u32,
    /// `json`, `markdown` or `none` (fragments and the shared system prompt's own shape).
    pub answer: &'static str,
    /// `fast`, `smart` or `none`.
    pub profile: &'static str,
    pub body: &'static str,
}

/// The prompt files of a product: `(id, file text)`.
pub type Sources = &'static [(&'static str, &'static str)];

/// Parses one prompt file; `None` if its header is malformed.
pub fn parse(source: &'static str) -> Option<Prompt> {
    let rest = source.strip_prefix("---\n")?;
    let (head, body) = rest.split_once("\n---\n")?;
    let mut prompt = Prompt {
        id: "",
        version: 0,
        answer: "",
        profile: "",
        body: body.strip_suffix('\n').unwrap_or(body),
    };
    for line in head.lines() {
        let (key, value) = line.split_once(':')?;
        let value = value.split('#').next().unwrap_or("").trim();
        match key.trim() {
            "id" => prompt.id = value,
            "version" => prompt.version = value.parse().ok()?,
            "answer" => prompt.answer = value,
            "profile" => prompt.profile = value,
            _ => return None,
        }
    }
    (!prompt.id.is_empty() && prompt.version > 0 && !prompt.answer.is_empty()).then_some(prompt)
}

/// All prompts of `sources`; `None` entries cannot happen in a build that passes the tests.
pub fn all(sources: Sources) -> Vec<Prompt> {
    sources.iter().filter_map(|(_, s)| parse(s)).collect()
}

/// The prompt `id` of `sources`.
pub fn get(sources: Sources, id: &str) -> Option<Prompt> {
    sources
        .iter()
        .find(|(i, _)| *i == id)
        .and_then(|(_, s)| parse(s))
}

/// The prompt `id` with `values` inserted for `<<name>>` and fragments for `<<include:id>>`.
/// Unknown names stay as written.
pub fn render(sources: Sources, id: &str, values: &[(&str, &str)]) -> String {
    let Some(prompt) = get(sources, id) else {
        return String::new();
    };
    expand(sources, prompt.body, values, 0)
}

fn expand(sources: Sources, template: &str, values: &[(&str, &str)], depth: usize) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("<<") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find(">>") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        let known = if let Some(fragment) = name.strip_prefix("include:") {
            // Fragments hold no values and no further fragments.
            get(sources, fragment)
                .filter(|_| depth == 0)
                .map(|p| expand(sources, p.body, &[], depth + 1))
        } else {
            values
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_owned())
        };
        match known {
            Some(text) => {
                out.push_str(&text);
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("<<");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCES: Sources = &[
        (
            "frag",
            "---\nid: frag\nversion: 1\nanswer: none\n---\nF<<x>>\n",
        ),
        (
            "main",
            "---\nid: main\nversion: 2\nanswer: json # c\nprofile: fast\n---\nA <<name>> <<include:frag>> <<nope>>\n",
        ),
    ];

    #[test]
    fn values_and_fragments_are_inserted_in_one_pass() {
        let p = get(SOURCES, "main").unwrap();
        assert_eq!((p.version, p.answer, p.profile), (2, "json", "fast"));
        // Values are not re-scanned; fragments hold no values.
        assert_eq!(
            render(SOURCES, "main", &[("name", "<<include:frag>>")]),
            "A <<include:frag>> F<<x>> <<nope>>"
        );
        assert_eq!(render(SOURCES, "missing", &[]), "");
        assert_eq!(all(SOURCES).len(), 2);
    }
}

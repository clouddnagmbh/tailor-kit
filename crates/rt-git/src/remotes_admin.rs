// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Remotes for the wizards (wave 14, strand G): list, add, remove, rename, change the
//! address; publish a branch and fetch. Only the repository's own `.git/config` is written.

use std::collections::BTreeMap;

use gix::ObjectId;
use gix::bstr::{BStr, ByteSlice};
use gix::refs::transaction::PreviousValue;
use rt_app::git::actions::{GitBlockReason, GitPlan, GitRefMove, GitRemote, check_ref_name};

use crate::actions::{Ctx, Done, PlanExt, Run, block};
use crate::repo::Repo;
use crate::util::{Context, Result, fail};

/// `url` without user name and password.
pub(crate) fn without_credentials(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        let (auth, tail) = match rest.find('/') {
            Some(i) => rest.split_at(i),
            None => (rest, ""),
        };
        if let Some(at) = auth.rfind('@') {
            return format!("{scheme}://{}{tail}", &auth[at + 1..]);
        }
    }
    url.to_owned()
}

impl Repo {
    /// The configured remotes, by name.
    pub fn remotes(&self) -> Result<Vec<GitRemote>> {
        let mut out = Vec::new();
        for name in self.repo.remote_names() {
            let name = name.to_str_lossy().into_owned();
            let Some(url) = self.config(&format!("remote.{name}.url")) else {
                continue;
            };
            out.push(GitRemote {
                url: without_credentials(&url),
                push_url: self
                    .config(&format!("remote.{name}.pushurl"))
                    .map(|u| without_credentials(&u)),
                name,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub(crate) fn has_remote(&self, name: &str) -> bool {
        !name.is_empty() && self.config(&format!("remote.{name}.url")).is_some()
    }

    /// Every remote-tracking ref (without `<remote>/HEAD`) and its commit.
    pub(crate) fn remote_refs(&self) -> Result<BTreeMap<String, ObjectId>> {
        let mut out = BTreeMap::new();
        let refs = self.repo.references().ctx("cannot list branches")?;
        for r in refs.remote_branches().ctx("cannot list branches")? {
            let mut r = r.map_err(|e| fail(format!("cannot list branches: {e}")))?;
            let name = r.name().as_bstr().to_str_lossy().into_owned();
            if name.ends_with("/HEAD") {
                continue;
            }
            if let Ok(id) = r.peel_to_id() {
                out.insert(name, id.detach());
            }
        }
        Ok(out)
    }

    /// Changes the repository's own `.git/config` with `f`.
    pub(crate) fn edit_config(
        &self,
        f: impl FnOnce(&mut gix::config::File) -> Result<()>,
    ) -> Result<()> {
        let path = self.repo.common_dir().join("config");
        let mut file =
            gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)
                .ctx("cannot read the repository configuration")?;
        f(&mut file)?;
        let mut out = Vec::new();
        file.write_to(&mut out)
            .ctx("cannot write the repository configuration")?;
        let text = String::from_utf8(out).map_err(|_| fail("the configuration is not UTF-8"))?;
        self.write_config_text(&text)
    }

    /// Removes `[section "sub"]` (every copy of it).
    pub(crate) fn remove_config_section(&self, section: &str, sub: &str) -> Result<()> {
        self.edit_config(|f| {
            while f.remove_section(section, sub).is_some() {}
            Ok(())
        })
    }

    // ---- add, remove, rename, set-url --------------------------------------------------

    pub(crate) fn plan_add_remote(&self, mut p: GitPlan, name: &str, url: &str) -> Result<GitPlan> {
        let url = url.trim();
        p.stop(check_ref_name(name).err());
        if self.has_remote(name) {
            p.stop(Some(block(
                GitBlockReason::Exists,
                format!("A remote named {name} exists already."),
            )));
        }
        if url.is_empty() {
            p.stop(Some(block(
                GitBlockReason::InvalidName,
                "Enter the address of the repository on the server.",
            )));
        }
        p.steps.push(format!(
            "Links this repository to {} as \"{name}\".",
            if url.is_empty() { "the server" } else { url }
        ));
        Ok(p)
    }

    pub(crate) fn run_add_remote(&self, name: &str, url: &str) -> Result<Done> {
        let url = url.trim();
        let fetch = format!("+refs/heads/*:refs/remotes/{name}/*");
        self.set_local_config(&[
            ("remote", name, "url", url),
            ("remote", name, "fetch", &fetch),
        ])?;
        Ok(Done::new(format!("Linked to {url} as {name}."), false))
    }

    pub(crate) fn plan_remove_remote(&self, mut p: GitPlan, name: &str) -> Result<GitPlan> {
        if !self.has_remote(name) {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no remote {name}."),
            )));
        }
        p.steps.push(format!(
            "Removes the link to {name}. Nothing on the server changes."
        ));
        Ok(p)
    }

    /// Short names of branches whose upstream is on `remote`.
    fn branches_on(file: &gix::config::File, remote: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(sections) = file.sections_by_name("branch") {
            for s in sections {
                if let Some(sub) = s.header().subsection_name()
                    && s.value("remote").is_some_and(|v| v == remote)
                {
                    out.push(sub.to_str_lossy().into_owned());
                }
            }
        }
        out
    }

    pub(crate) fn run_remove_remote(&self, name: &str) -> Result<Done> {
        self.edit_config(|f| {
            while f.remove_section("remote", name).is_some() {}
            for branch in Self::branches_on(f, name) {
                if let Ok(mut s) = f.section_mut("branch", Some(BStr::new(branch.as_str()))) {
                    s.remove("remote");
                    s.remove("merge");
                }
            }
            Ok(())
        })?;
        let prefix = format!("refs/remotes/{name}/");
        for (r, id) in self.remote_refs()? {
            if r.starts_with(&prefix) {
                self.put_ref(&r, None, Some(id), "remote: removed")?;
            }
        }
        Ok(Done::new(format!("Removed the remote {name}."), false))
    }

    pub(crate) fn plan_rename_remote(
        &self,
        mut p: GitPlan,
        from: &str,
        to: &str,
    ) -> Result<GitPlan> {
        if !self.has_remote(from) {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no remote {from}."),
            )));
        }
        p.stop(check_ref_name(to).err());
        if self.has_remote(to) {
            p.stop(Some(block(
                GitBlockReason::Exists,
                format!("A remote named {to} exists already."),
            )));
        }
        p.steps.push(format!("Renames the remote {from} to {to}."));
        Ok(p)
    }

    pub(crate) fn run_rename_remote(&self, from: &str, to: &str) -> Result<Done> {
        let (old_prefix, new_prefix) = (
            format!("refs/remotes/{from}/"),
            format!("refs/remotes/{to}/"),
        );
        self.edit_config(|f| {
            f.rename_section("remote", from, "remote", to.to_owned())
                .ctx("cannot rename the remote")?;
            let fetch = f
                .string(&format!("remote.{to}.fetch"))
                .map(|v| v.to_str_lossy().into_owned());
            if let Some(fetch) = fetch
                && fetch.contains(&old_prefix)
            {
                let new = fetch.replace(&old_prefix, &new_prefix);
                f.set_raw_value_by("remote", Some(BStr::new(to)), "fetch", BStr::new(&new))
                    .ctx("cannot rename the remote")?;
            }
            for branch in Self::branches_on(f, from) {
                f.set_raw_value_by(
                    "branch",
                    Some(BStr::new(branch.as_str())),
                    "remote",
                    BStr::new(to),
                )
                .ctx("cannot rename the remote")?;
            }
            Ok(())
        })?;
        for (r, id) in self.remote_refs()? {
            if let Some(rest) = r.strip_prefix(&old_prefix) {
                let new = format!("{new_prefix}{rest}");
                self.repo
                    .reference(new.as_str(), id, PreviousValue::Any, "remote: renamed")
                    .ctx("cannot rename a remote branch")?;
                self.put_ref(&r, None, Some(id), "remote: renamed")?;
            }
        }
        Ok(Done::new(
            format!("Renamed the remote {from} to {to}."),
            false,
        ))
    }

    pub(crate) fn plan_set_remote_url(
        &self,
        mut p: GitPlan,
        name: &str,
        url: &str,
    ) -> Result<GitPlan> {
        let url = url.trim();
        if !self.has_remote(name) {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no remote {name}."),
            )));
        }
        if url.is_empty() {
            p.stop(Some(block(
                GitBlockReason::InvalidName,
                "Enter the address of the repository on the server.",
            )));
        }
        p.steps
            .push(format!("Changes the address of {name} to {url}."));
        Ok(p)
    }

    pub(crate) fn run_set_remote_url(&self, name: &str, url: &str) -> Result<Done> {
        let url = url.trim();
        self.set_local_config(&[("remote", name, "url", url)])?;
        Ok(Done::new(format!("{name} now points to {url}."), false))
    }

    // ---- publish, fetch ----------------------------------------------------------------

    pub(crate) fn plan_publish(
        &self,
        cx: &Ctx,
        mut p: GitPlan,
        branch: &str,
        remote: &str,
    ) -> Result<GitPlan> {
        p.network = true;
        p.undoable = false;
        let full = format!("refs/heads/{branch}");
        let at = if check_ref_name(branch).is_ok() {
            self.ref_value(&full)?
        } else {
            None
        };
        let unborn = at.is_none() && cx.head_ref.as_ref().is_some_and(|h| h.as_bstr() == full);
        if unborn {
            p.stop(Some(block(
                GitBlockReason::NothingToDo,
                "Make the first commit first.",
            )));
        } else if at.is_none() {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no branch {branch}."),
            )));
        }
        if !self.has_remote(remote) {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no remote {remote}. Add it first."),
            )));
        }
        if self.config(&format!("branch.{branch}.merge")).is_some() {
            p.stop(Some(block(
                GitBlockReason::NothingToDo,
                format!("{branch} is on the server already."),
            )));
        }
        p.steps.push(format!(
            "Puts {branch} on {remote} and links the two, so Pull and Push work from now on."
        ));
        let tracking = format!("refs/remotes/{remote}/{branch}");
        p.refs.push(GitRefMove {
            before: self.ref_value(&tracking)?.map(|id| id.to_string()),
            name: tracking,
            after: at.map(|id| id.to_string()),
        });
        Ok(p)
    }

    pub(crate) fn run_publish(
        &self,
        run: &mut Run<'_>,
        branch: &str,
        remote: &str,
    ) -> Result<Done> {
        let full = format!("refs/heads/{branch}");
        let at = self
            .ref_value(&full)?
            .ok_or_else(|| fail(format!("There is no branch {branch}.")))?;
        self.push_ref(remote, at, &full, run.opts.credentials)?;
        self.repo
            .reference(
                format!("refs/remotes/{remote}/{branch}"),
                at,
                PreviousValue::Any,
                "update by push",
            )
            .ctx("cannot update the remote-tracking branch")?;
        self.set_local_config(&[
            ("branch", branch, "remote", remote),
            ("branch", branch, "merge", &full),
        ])?;
        Ok(Done::new(format!("Published {branch} to {remote}."), false))
    }

    /// The remote a fetch without a name asks: the upstream's, else `origin`, else the only one.
    fn default_remote(&self) -> Result<String> {
        if let Some(up) = self.upstream()? {
            return Ok(up.remote);
        }
        if self.has_remote("origin") {
            return Ok("origin".into());
        }
        let remotes = self.remotes()?;
        Ok(match remotes.as_slice() {
            [one] => one.name.clone(),
            _ => "origin".into(),
        })
    }

    pub(crate) fn plan_fetch(&self, mut p: GitPlan, remote: Option<&str>) -> Result<GitPlan> {
        p.network = true;
        p.undoable = false;
        let name = match remote {
            Some(r) => r.to_owned(),
            None => self.default_remote()?,
        };
        if !self.has_remote(&name) {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no remote {name}."),
            )));
        }
        p.steps.push(format!(
            "Asks {name} what is new. Your branches and files stay as they are."
        ));
        Ok(p)
    }

    pub(crate) fn run_fetch(&self, run: &mut Run<'_>, remote: Option<&str>) -> Result<Done> {
        let name = match remote {
            Some(r) => r.to_owned(),
            None => self.default_remote()?,
        };
        self.fetch(&name, run.opts.credentials, false)?;
        Ok(Done::new("Checked the server for changes.", false))
    }
}

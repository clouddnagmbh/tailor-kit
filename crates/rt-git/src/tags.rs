// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Tags for the wizards (wave 14, strand G): create (annotated with a message, else
//! lightweight), delete (with a backup), push to a remote.

use gix::refs::transaction::PreviousValue;
use rt_app::git::actions::{GitBlockReason, GitPlan, GitRefMove, check_ref_name};

use crate::actions::{Done, PlanExt, Run, block, short};
use crate::commit::signature;
use crate::repo::Repo;
use crate::util::{Context, Result, fail};

fn tag_full(name: &str) -> String {
    format!("refs/tags/{name}")
}

impl Repo {
    fn tag_value(&self, name: &str) -> Result<Option<gix::ObjectId>> {
        if check_ref_name(name).is_err() {
            return Ok(None);
        }
        self.ref_value(&tag_full(name))
    }

    pub(crate) fn plan_create_tag(
        &self,
        mut p: GitPlan,
        name: &str,
        target: &str,
        message: Option<&str>,
    ) -> Result<GitPlan> {
        let to = self.try_resolve(target);
        p.stop(check_ref_name(name).err());
        if self.tag_value(name)?.is_some() {
            p.stop(Some(block(
                GitBlockReason::Exists,
                format!("A tag named {name} exists already."),
            )));
        }
        if to.is_none() {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no commit \"{target}\"."),
            )));
        }
        let annotated = message.is_some_and(|m| !m.trim().is_empty());
        p.steps.push(format!(
            "Names {} \"{name}\"{}. A tag never moves.",
            to.map(|id| short(&id)).unwrap_or_else(|| target.to_owned()),
            if annotated { " with a message" } else { "" }
        ));
        p.refs.push(GitRefMove {
            name: tag_full(name),
            before: None,
            after: to.map(|id| id.to_string()),
        });
        Ok(p)
    }

    pub(crate) fn run_create_tag(
        &self,
        run: &mut Run<'_>,
        name: &str,
        target: &str,
        message: Option<&str>,
    ) -> Result<Done> {
        let to = self.resolve_rev(target)?;
        match message.map(str::trim).filter(|m| !m.is_empty()) {
            Some(message) => {
                let identity = self.identity_or(run.opts.identity)?;
                let tagger = signature(&identity);
                let mut time = gix::date::parse::TimeBuf::default();
                self.repo
                    .tag(
                        name,
                        to,
                        gix::objs::Kind::Commit,
                        Some(tagger.to_ref(&mut time)),
                        message,
                        PreviousValue::MustNotExist,
                    )
                    .ctx("cannot create the tag")?;
            }
            None => {
                self.put_ref(&tag_full(name), Some(to), None, "tag: created")?;
            }
        }
        Ok(Done::new(
            format!("Tagged {} as {name}.", short(&to)),
            false,
        ))
    }

    pub(crate) fn plan_delete_tag(&self, mut p: GitPlan, name: &str) -> Result<GitPlan> {
        let Some(at) = self.tag_value(name)? else {
            return Ok(p.blocked_by(block(
                GitBlockReason::NotFound,
                format!("There is no tag {name}."),
            )));
        };
        p.steps
            .push(format!("Deletes the tag {name} on this computer."));
        let on_server = self.remotes()?.iter().any(|r| {
            self.remote_ref_offline(&r.name, &tag_full(name))
                .is_some_and(|v| v.is_some())
        });
        if on_server {
            p.warnings
                .push("The server keeps its copy of the tag.".into());
        }
        p.refs.push(GitRefMove {
            name: tag_full(name),
            before: Some(at.to_string()),
            after: None,
        });
        p.backup = true;
        Ok(p)
    }

    pub(crate) fn run_delete_tag(&self, run: &mut Run<'_>, name: &str) -> Result<Done> {
        let at = self.tag_value(name)?;
        run.backup(self, &format!("tags/{name}"), at)?;
        self.put_ref(&tag_full(name), None, at, "tag: deleted")?;
        Ok(Done::new(format!("Deleted the tag {name}."), false))
    }

    pub(crate) fn plan_push_tag(
        &self,
        mut p: GitPlan,
        name: &str,
        remote: &str,
    ) -> Result<GitPlan> {
        p.network = true;
        p.undoable = false;
        let at = self.tag_value(name)?;
        if at.is_none() {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no tag {name}."),
            )));
        }
        if !self.has_remote(remote) {
            p.stop(Some(block(
                GitBlockReason::NotFound,
                format!("There is no remote {remote}."),
            )));
        }
        match self.remote_ref_offline(remote, &tag_full(name)) {
            Some(Some(there)) if Some(there) == at => p.stop(Some(block(
                GitBlockReason::NothingToDo,
                format!("{remote} has the tag already."),
            ))),
            Some(Some(_)) => p.stop(Some(block(
                GitBlockReason::Rejected,
                format!("{remote} has another tag named {name}. Tags never move, so it stays."),
            ))),
            _ => {}
        }
        p.steps.push(format!("Sends the tag {name} to {remote}."));
        Ok(p)
    }

    pub(crate) fn run_push_tag(&self, run: &mut Run<'_>, name: &str, remote: &str) -> Result<Done> {
        let at = self
            .tag_value(name)?
            .ok_or_else(|| fail(format!("There is no tag {name}.")))?;
        self.push_ref(remote, at, &tag_full(name), run.opts.credentials)?;
        Ok(Done::new(
            format!("Sent the tag {name} to {remote}."),
            false,
        ))
    }
}

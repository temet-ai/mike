#[cfg(test)]
mod acceptance_artifacts_tests;
#[cfg(test)]
mod brief_note_tests;
mod claude_args;
#[cfg(test)]
mod profile_tests;
mod split;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;

use uuid::Uuid;

pub use split::{SplitLaunchArgs, split_launch_args};

use super::brief::{self, BriefMode, BriefStaging, StagedBrief};
use super::env::{ProxyEnv, surface_env};
use super::error::LaunchError;
use claude_args::{SESSION_PLACEHOLDER, SessionRule};

#[derive(Debug, Clone)]
pub struct AssembleOptions<'a> {
    pub program: String,
    pub proxy: Option<&'a ProxyEnv>,
    pub brief_inline_max: usize,
    pub briefs_dir: &'a Path,
    /// The launch ledger is reachable, so a brief can become a session note.
    pub brief_notes: bool,
    /// The proxy attaches note-mode briefs (the launch_brief toggle), so a
    /// long brief can travel as a pointer instead of a file.
    pub brief_by_mail: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub session: Option<Uuid>,
    pub preassigned: bool,
    pub program: String,
    pub args: Vec<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub prompt: Option<String>,
    pub brief: Option<StagedBrief>,
    pub title: String,
    pub env: BTreeMap<String, String>,
    pub agent_name: String,
    pub flags: SplitLaunchArgs,
}

impl LaunchSpec {
    pub fn assemble(
        split: SplitLaunchArgs,
        prompt: Option<String>,
        opts: &AssembleOptions<'_>,
    ) -> Result<Self, LaunchError> {
        let (session, preassigned) = match claude_args::session_rule(&split.claude_args)? {
            SessionRule::Explicit(id) | SessionRule::Resumed(id) => (Some(id), false),
            SessionRule::Resume => (None, false),
            SessionRule::Preassign => (Some(Uuid::new_v4()), true),
        };
        if prompt.is_some() && claude_args::bare_prompt_present(&split.claude_args) {
            return Err(LaunchError::Usage(
                "--prompt-file cannot be combined with a bare prompt argument; pass one or the other".into(),
            ));
        }
        let caller_args = substitute_all(&split.claude_args, session)?;
        let prompt = prompt.map(|p| substitute_text(&p, session)).transpose()?;
        let acceptance =
            super::fleet::acceptance_artifacts::for_launch(opts.briefs_dir, &split, session)?;
        let prompt = with_acceptance_reference(prompt, &split, acceptance.as_deref());
        let file_id = session.unwrap_or_else(Uuid::new_v4);
        let staging = BriefStaging {
            briefs_dir: opts.briefs_dir,
            inline_max: opts.brief_inline_max,
            notes: opts.brief_notes,
            by_mail: opts.brief_by_mail,
            who: split.session_name.as_deref().or(split.seat.as_deref()),
        };
        let brief = prompt
            .as_deref()
            .map(|p| brief::stage(p, session, file_id, &staging))
            .transpose()?;

        let mut args = Vec::with_capacity(caller_args.len() + 4);
        if let (Some(id), true) = (session, preassigned) {
            args.push("--session-id".to_string());
            args.push(id.to_string());
        }
        args.extend(caller_args);
        if split.remote_control && !claude_args::has_remote_control(&split.claude_args) {
            args.push("--remote-control".to_string());
        }
        if split.channels && !claude_args::has_channels(&split.claude_args) {
            args.push(super::DEV_CHANNELS_FLAG.to_string());
            args.push("server:mike".to_string());
        }
        if let Some(staged) = brief.as_ref().filter(|b| b.mode != BriefMode::Note) {
            args.extend(read_permission(staged));
            args.push(staged.delivered.clone());
        }

        let model = claude_args::read_value(&split.claude_args, "--model");
        let agent = claude_args::read_value(&split.claude_args, "--agent");
        let mut env = surface_env(opts.proxy, split.proxy_env);
        seat_env(&mut env, session, &split);
        if let Some(path) = acceptance {
            env.insert(
                super::fleet::acceptance_artifacts::MANIFEST_ENV.to_string(),
                path.to_string_lossy().into_owned(),
            );
        }
        Ok(Self {
            session,
            preassigned,
            program: opts.program.clone(),
            args,
            title: claude_args::title(model.as_deref(), prompt.as_deref()),
            model,
            agent,
            prompt,
            brief,
            env,
            agent_name: agent_name(file_id),
            flags: split,
        })
    }
}

fn read_permission(staged: &StagedBrief) -> Option<String> {
    let path = staged
        .path
        .as_ref()
        .filter(|_| staged.mode == BriefMode::File)?;
    let path = path.to_string_lossy();
    let absolute = path.strip_prefix('/')?;
    Some(format!("--allowedTools=Read(//{absolute})"))
}

fn with_acceptance_reference(
    prompt: Option<String>,
    split: &SplitLaunchArgs,
    manifest: Option<&Path>,
) -> Option<String> {
    let Some(manifest) = manifest else {
        return prompt;
    };
    let reference = super::fleet::acceptance_artifacts::brief_reference(manifest);
    match prompt {
        Some(prompt) => Some(format!("{prompt}\n\n{reference}")),
        None if split.claude_args.iter().any(|arg| !arg.starts_with('-')) => None,
        None => Some(reference),
    }
}

impl LaunchSpec {
    /// The launch row that would carry this spec's note-mode brief was not
    /// written, so nothing could deliver it: stage it to a file instead and
    /// swap the pointer on the argv. Inline briefs only lose their note.
    pub fn demote_brief_to_file(&mut self, briefs_dir: &Path) -> Result<(), LaunchError> {
        let (Some(staged), Some(prompt)) = (&self.brief, &self.prompt) else {
            return Ok(());
        };
        let file_id = self.session.unwrap_or_else(Uuid::new_v4);
        let fallback = brief::FileFallback {
            file_id,
            briefs_dir,
            who: self
                .flags
                .session_name
                .as_deref()
                .or(self.flags.seat.as_deref()),
        };
        let demoted = brief::demote_to_file(staged, prompt, &fallback)?;
        let typed = staged.mode != BriefMode::Note;
        let changed = demoted.delivered != staged.delivered;
        if !typed || (changed && self.args.pop().is_some()) {
            self.args.extend(read_permission(&demoted));
            self.args.push(demoted.delivered.clone());
        }
        self.brief = Some(demoted);
        Ok(())
    }
}

pub const SHELL_RETURNS_TO_THE_CHECKOUT: &str = "CLAUDE_BASH_MAINTAIN_PROJECT_WORKING_DIR";

fn seat_env(env: &mut BTreeMap<String, String>, session: Option<Uuid>, split: &SplitLaunchArgs) {
    if split
        .run
        .as_deref()
        .is_some_and(|run| run.starts_with("fleet-"))
        && let Some(kind) = split.role.as_deref().or(split.seat.as_deref())
    {
        env.insert(
            super::fleet::PROFILE_ENV.to_string(),
            super::fleet::profile_for(kind).to_string(),
        );
    }
    if let Some(session) = session {
        env.insert("MIKE_SESSION_ID".to_string(), session.to_string());
    }
    if let Some(run) = &split.run {
        env.insert("MIKE_RUN_ID".to_string(), run.clone());
    }
    if let Some(seat) = &split.seat {
        env.insert("MIKE_SEAT".to_string(), seat.clone());
        env.insert(SHELL_RETURNS_TO_THE_CHECKOUT.to_string(), "1".to_string());
    }
}

pub fn agent_name(id: Uuid) -> String {
    let simple = id.simple().to_string();
    format!("cc-{}", &simple[..8])
}

fn substitute_all(args: &[String], session: Option<Uuid>) -> Result<Vec<String>, LaunchError> {
    args.iter().map(|a| substitute_text(a, session)).collect()
}

fn substitute_text(text: &str, session: Option<Uuid>) -> Result<String, LaunchError> {
    match session {
        Some(id) => Ok(claude_args::substitute_session(text, id)),
        None if text.contains(SESSION_PLACEHOLDER) => Err(LaunchError::Usage(format!(
            "{SESSION_PLACEHOLDER} was used but this launch has no session id (a --resume/--continue \
             launch without --session-id)"
        ))),
        None => Ok(text.to_string()),
    }
}

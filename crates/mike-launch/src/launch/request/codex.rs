use std::collections::BTreeMap;

use mike_codex::interactive::parse_interactive;
use uuid::Uuid;

use super::{
    AssembleOptions, LaunchError, LaunchSpec, SplitLaunchArgs, seat_env, substitute_all,
    substitute_text,
};
use crate::launch::brief::{self, BriefMode, BriefStaging};

pub(super) fn assemble(
    mut split: SplitLaunchArgs,
    prompt: Option<String>,
    opts: &AssembleOptions<'_>,
) -> Result<LaunchSpec, LaunchError> {
    if split.template_id.is_some() {
        return Err(LaunchError::Usage(
            "Codex spawn cannot use a Claude warm template".into(),
        ));
    }
    if split.trust {
        return Err(LaunchError::Usage(
            "Codex reviews its own project and hook trust; omit --trust".into(),
        ));
    }
    let parsed = parse_interactive(&split.claude_args)
        .map_err(|error| LaunchError::Usage(error.to_string()))?;
    if prompt.is_some() && parsed.prompt.is_some() {
        return Err(LaunchError::Usage(
            "--prompt-file cannot be combined with a bare prompt argument".into(),
        ));
    }
    split.remote_control = false;
    split.channels = false;
    let session = Uuid::new_v4();
    let prompt = prompt
        .or(parsed.prompt)
        .map(|prompt| substitute_text(&prompt, Some(session)))
        .transpose()?;
    let mut args = substitute_all(&parsed.args, Some(session))?;
    let staging = BriefStaging {
        briefs_dir: opts.briefs_dir,
        inline_max: opts.brief_inline_max,
        notes: false,
        by_mail: false,
        who: split.session_name.as_deref().or(split.seat.as_deref()),
    };
    let mut brief = prompt
        .as_deref()
        .map(|prompt| brief::stage(prompt, Some(session), session, &staging))
        .transpose()?;
    if let Some(brief) = &mut brief {
        if let Some(path) = brief
            .path
            .as_ref()
            .filter(|_| brief.mode == BriefMode::File)
        {
            brief.delivered = format!(
                "Read your complete brief at {} and carry it out.",
                path.display()
            );
        }
        brief.note = opts.brief_notes.then(Uuid::new_v4);
        if !args.iter().any(|arg| arg == "--") {
            args.push("--".into());
        }
        args.push(brief.delivered.clone());
    }
    let mut env = BTreeMap::new();
    seat_env(&mut env, Some(session), &split);
    env.remove(super::SHELL_RETURNS_TO_THE_CHECKOUT);
    env.insert("MIKE_CODEX_SPAWN_SESSION_ID".into(), session.to_string());
    let title = split
        .session_name
        .clone()
        .or_else(|| {
            parsed
                .model
                .as_deref()
                .map(|model| format!("codex · {model}"))
        })
        .unwrap_or_else(|| "codex".into());
    Ok(LaunchSpec {
        session: Some(session),
        preassigned: true,
        program: "codex".into(),
        args,
        model: parsed.model,
        agent: None,
        prompt,
        brief,
        title,
        env,
        agent_name: format!("codex-{}", &session.simple().to_string()[..8]),
        flags: split,
    })
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;

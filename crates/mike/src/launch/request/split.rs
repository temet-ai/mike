use super::super::backend::{Placement, SplitDirection};
use super::super::brief::PromptSource;
use super::super::error::LaunchError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitLaunchArgs {
    pub placement: Option<Placement>,
    pub anchor_terminal: Option<String>,
    pub json: bool,
    pub worktree_ok: bool,
    pub carry_config: bool,
    pub prompt_file: Option<PromptSource>,
    pub run: Option<String>,
    pub seat: Option<String>,
    pub role: Option<String>,
    pub instructions: Option<String>,
    pub goal: Option<String>,
    pub session_name: Option<String>,
    pub template_id: Option<i64>,
    pub trust: bool,
    pub proxy_env: bool,
    pub remote_control: bool,
    pub channels: bool,
    pub claude_args: Vec<String>,
}

impl SplitLaunchArgs {
    fn empty() -> Self {
        Self {
            placement: None,
            anchor_terminal: None,
            json: false,
            worktree_ok: false,
            carry_config: true,
            prompt_file: None,
            run: None,
            seat: None,
            role: None,
            instructions: None,
            goal: None,
            session_name: None,
            template_id: None,
            trust: false,
            proxy_env: true,
            remote_control: true,
            channels: true,
            claude_args: Vec::new(),
        }
    }
}

enum Flag {
    Tab,
    Split,
    Panel,
    AnchorTerminal,
    Json,
    WorktreeOk,
    NoCarryConfig,
    PromptFile,
    Run,
    Seat,
    Role,
    Instructions,
    Goal,
    SessionName,
    TemplateId,
    Trust,
    NoProxyEnv,
    NoRemoteControl,
    NoChannels,
}

impl Flag {
    fn parse(token: &str) -> Option<Flag> {
        Some(match token {
            "--tab" => Flag::Tab,
            "--split" => Flag::Split,
            "--panel" => Flag::Panel,
            "--anchor-terminal" => Flag::AnchorTerminal,
            "--json" => Flag::Json,
            "--worktree-ok" => Flag::WorktreeOk,
            "--no-carry-config" => Flag::NoCarryConfig,
            "--prompt-file" => Flag::PromptFile,
            "--run" => Flag::Run,
            "--seat" => Flag::Seat,
            "--role" => Flag::Role,
            "--instructions" => Flag::Instructions,
            "--goal" => Flag::Goal,
            "--session-name" => Flag::SessionName,
            "--template-id" => Flag::TemplateId,
            "--trust" => Flag::Trust,
            "--no-proxy-env" => Flag::NoProxyEnv,
            "--no-remote-control" => Flag::NoRemoteControl,
            "--no-channels" => Flag::NoChannels,
            _ => return None,
        })
    }

    fn takes_value(&self) -> bool {
        matches!(
            self,
            Flag::Split
                | Flag::AnchorTerminal
                | Flag::PromptFile
                | Flag::Run
                | Flag::Seat
                | Flag::Role
                | Flag::Instructions
                | Flag::Goal
                | Flag::SessionName
                | Flag::TemplateId
        )
    }
}

pub fn split_launch_args(
    tokens: impl IntoIterator<Item = String>,
) -> Result<SplitLaunchArgs, LaunchError> {
    let mut out = SplitLaunchArgs::empty();
    let mut iter = tokens.into_iter();
    while let Some(token) = iter.next() {
        if token == "--" {
            out.claude_args.extend(iter);
            break;
        }
        reject_legacy(&token)?;
        let (name, inline_value) = split_inline_value(&token);
        let Some(flag) = Flag::parse(name) else {
            out.claude_args.push(token);
            continue;
        };
        let value = if flag.takes_value() {
            match inline_value.map(str::to_string).or_else(|| iter.next()) {
                Some(v) => Some(v),
                None => return Err(LaunchError::Usage(format!("{name} needs a value"))),
            }
        } else {
            if inline_value.is_some() {
                return Err(LaunchError::Usage(format!("{name} takes no value")));
            }
            None
        };
        apply(&mut out, flag, name, value)?;
    }
    Ok(out)
}

fn split_inline_value(token: &str) -> (&str, Option<&str>) {
    match token.split_once('=') {
        Some((name, value)) if name.starts_with("--") => (name, Some(value)),
        _ => (token, None),
    }
}

fn reject_legacy(token: &str) -> Result<(), LaunchError> {
    match token {
        "-m" => Err(LaunchError::Usage(
            "the legacy `-m <model>` form is not supported; pass `--model <model>` as a claude argument".into(),
        )),
        "-p" | "--prompt" => Err(LaunchError::Usage(
            "the legacy `-p`/`--prompt` form is not supported and print mode is out of scope; \
             pass the prompt as a claude argument or use `--prompt-file <path|->`"
                .into(),
        )),
        _ => Ok(()),
    }
}

fn apply(
    out: &mut SplitLaunchArgs,
    flag: Flag,
    name: &str,
    value: Option<String>,
) -> Result<(), LaunchError> {
    match flag {
        Flag::Tab => set_placement(out, Placement::Tab, name)?,
        Flag::Split => {
            let dir: SplitDirection = value.unwrap_or_default().parse()?;
            set_placement(out, Placement::Split(dir), name)?;
        }
        Flag::Panel => set_placement(out, Placement::Panel, name)?,
        Flag::AnchorTerminal => out.anchor_terminal = value,
        Flag::Json => out.json = true,
        Flag::WorktreeOk => out.worktree_ok = true,
        Flag::NoCarryConfig => out.carry_config = false,
        Flag::PromptFile => out.prompt_file = value.as_deref().map(PromptSource::parse),
        Flag::Run => out.run = value,
        Flag::Seat => out.seat = value,
        Flag::Role => out.role = value,
        Flag::Instructions => out.instructions = value,
        Flag::Goal => out.goal = value,
        Flag::SessionName => out.session_name = value,
        Flag::TemplateId => out.template_id = Some(template_id(name, value)?),
        Flag::Trust => out.trust = true,
        Flag::NoProxyEnv => out.proxy_env = false,
        Flag::NoRemoteControl => out.remote_control = false,
        Flag::NoChannels => out.channels = false,
    }
    Ok(())
}

fn template_id(name: &str, value: Option<String>) -> Result<i64, LaunchError> {
    let text = value.unwrap_or_default();
    text.parse()
        .map_err(|_| LaunchError::Usage(format!("{name} `{text}` is not a template id")))
}

fn set_placement(
    out: &mut SplitLaunchArgs,
    placement: Placement,
    name: &str,
) -> Result<(), LaunchError> {
    let conflicting = matches!(
        (out.placement, placement),
        (Some(Placement::Tab), Placement::Split(_)) | (Some(Placement::Split(_)), Placement::Tab)
    );
    if conflicting {
        return Err(LaunchError::Usage(format!(
            "{name} cannot be combined with the other placement flag; --tab and --split are mutually exclusive"
        )));
    }
    out.placement = Some(placement);
    Ok(())
}

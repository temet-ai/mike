use crate::launch::{BriefMode, LaunchError, LaunchOutput, LaunchSpec, Surface};

pub const PROMPT_WAIT_MS: u64 = 30_000;

pub struct Delivery {
    pub args: Vec<String>,
    pub prompt: Option<String>,
}

impl Delivery {
    pub fn split(spec: &LaunchSpec, prompt_in_argv: bool) -> Self {
        match &spec.brief {
            Some(staged) if staged.mode == BriefMode::Note => Self {
                args: spec.args.clone(),
                prompt: None,
            },
            Some(staged) if !prompt_in_argv => {
                let mut args = spec.args.clone();
                if args.last() == Some(&staged.delivered) {
                    args.pop();
                }
                Self {
                    args,
                    prompt: Some(staged.delivered.clone()),
                }
            }
            _ => Self {
                args: spec.args.clone(),
                prompt: None,
            },
        }
    }
}

pub fn reason(output: &LaunchOutput) -> &'static str {
    if output.delivered == Some(false) {
        "prompt_undelivered"
    } else {
        "prompt_unconfirmed"
    }
}

pub fn unconfirmed_error(output: &LaunchOutput, surface: &Surface) -> LaunchError {
    let what = if output.delivered == Some(false) {
        "the prompt was not submitted"
    } else {
        "the prompt was submitted but not seen to become a turn"
    };
    let detail = surface
        .prompt_detail
        .as_deref()
        .map(|d| format!(" ({d})"))
        .unwrap_or_default();
    LaunchError::Unconfirmed(format!(
        "{what}{detail}; the surface {} is left open",
        surface.surface_ref
    ))
}

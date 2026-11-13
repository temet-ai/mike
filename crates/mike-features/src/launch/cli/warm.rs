use std::io::Read;

use uuid::Uuid;

use super::open::{self, OpenFailure, OpenResult};
use super::runner::Runner;
use crate::launch::{LaunchError, LaunchOutput};
use crate::store::persistence::launch::NewTemplate;

pub struct WarmOutcome {
    pub result: OpenResult,
    pub replaced_session: Option<Uuid>,
}

fn strip_name(tokens: Vec<String>) -> Result<(Vec<String>, String), LaunchError> {
    let mut name = None;
    let mut kept = Vec::with_capacity(tokens.len());
    let mut iter = tokens.into_iter();
    let mut verbatim = false;
    while let Some(token) = iter.next() {
        if !verbatim && token == "--name" {
            name = iter.next();
            continue;
        }
        if token == "--" {
            verbatim = true;
        }
        kept.push(token);
    }
    let name = name.ok_or_else(|| {
        LaunchError::Usage("warm needs --name <discipline>: mike spawn warm <folder> --name <discipline> [claude args...]".into())
    })?;
    if name.trim().is_empty() {
        return Err(LaunchError::Usage("--name cannot be empty".into()));
    }
    Ok((kept, name))
}

pub async fn execute(
    runner: &Runner<'_>,
    tokens: Vec<String>,
    stdin: &mut (dyn Read + Send),
) -> Result<WarmOutcome, OpenFailure> {
    let (tokens, name) = strip_name(tokens).map_err(|e| OpenFailure {
        output: None,
        error: e,
    })?;
    let mut prepared = open::prepare(runner, tokens, stdin).await?;
    prepared.default_role(&name);
    let claude_args = prepared.claude_args().to_vec();
    let folder = prepared.folder().to_string_lossy().into_owned();
    let backend = prepared.backend().as_str().to_string();

    let started = open::start(runner, prepared).await?;
    let session = started.session();
    let result = open::finish(runner, started).await;

    let replaced_session = match (mechanically_launched(&result), session) {
        (true, Some(session)) => {
            record_template(runner, &name, session, folder, backend, claude_args).await
        }
        _ => None,
    };
    Ok(WarmOutcome {
        result,
        replaced_session,
    })
}

fn mechanically_launched(result: &OpenResult) -> bool {
    let output: &LaunchOutput = match result {
        Ok(output) => output,
        Err(OpenFailure {
            output: Some(output),
            ..
        }) => output,
        Err(OpenFailure { output: None, .. }) => return false,
    };
    output.opened && output.delivered == Some(true)
}

async fn record_template(
    runner: &Runner<'_>,
    name: &str,
    session: Uuid,
    folder: String,
    backend: String,
    claude_args: Vec<String>,
) -> Option<Uuid> {
    let (model, agent) = (
        read_flag(&claude_args, "--model"),
        read_flag(&claude_args, "--agent"),
    );
    let new = NewTemplate {
        name: name.to_string(),
        session_uuid: session,
        folder,
        backend,
        model,
        agent,
        claude_args,
    };
    match runner.ledger.insert_template(&new).await {
        Ok(inserted) => inserted.replaced_session,
        Err(e) => {
            eprintln!("mike spawn warm: could not record the template: {e}");
            None
        }
    }
}

fn read_flag(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
        if let Some(value) = arg.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(any(test, feature = "test-seams"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use super::*;
    use crate::launch::{BackendId, ChannelCheck};

    fn output(opened: bool, delivered: Option<bool>) -> LaunchOutput {
        LaunchOutput {
            backend: BackendId::Tmux,
            workspace: None,
            folder: "/repo".into(),
            surface: None,
            session: Some(Uuid::nil()),
            reused: false,
            adopted: false,
            opened,
            delivered,
            confirmed: false,
            channel_check: ChannelCheck::Found,
            brief_mode: None,
            run: None,
            seat: None,
            reason: None,
        }
    }

    #[test]
    fn mechanically_launched_true_on_an_ok_result() {
        assert!(mechanically_launched(&Ok(output(true, Some(true)))));
    }

    #[test]
    fn mechanically_launched_true_on_an_unconfirmed_err_that_still_opened_and_delivered() {
        let result: OpenResult = Err(OpenFailure {
            output: Some(output(true, Some(true))),
            error: LaunchError::Unconfirmed("prompt not seen to become a turn".into()),
        });
        assert!(mechanically_launched(&result));
    }

    #[test]
    fn mechanically_launched_false_when_never_opened_or_no_output_at_all() {
        let not_opened: OpenResult = Err(OpenFailure {
            output: Some(output(false, None)),
            error: LaunchError::Refused("untrusted".into()),
        });
        assert!(!mechanically_launched(&not_opened));

        let no_output: OpenResult = Err(OpenFailure {
            output: None,
            error: LaunchError::Usage("bad args".into()),
        });
        assert!(!mechanically_launched(&no_output));
    }

    #[test]
    fn strip_name_extracts_wherever_it_appears() -> anyhow::Result<()> {
        let (kept, name) = strip_name(vec![
            "/repo".into(),
            "--tab".into(),
            "--name".into(),
            "verifier".into(),
            "--model".into(),
            "opus".into(),
        ])?;
        assert_eq!(name, "verifier");
        assert_eq!(kept, vec!["/repo", "--tab", "--model", "opus"]);
        Ok(())
    }

    #[test]
    fn strip_name_missing_is_usage_error() {
        let err = strip_name(vec!["/repo".into()]).unwrap_err();
        assert_eq!(err.exit_code(), 1);
        assert!(err.to_string().contains("--name"));
    }

    #[test]
    fn strip_name_empty_value_is_usage_error() {
        let err = strip_name(vec!["/repo".into(), "--name".into(), "".into()]).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn strip_name_after_double_dash_is_left_alone() -> anyhow::Result<()> {
        let (kept, name) = strip_name(vec![
            "/repo".into(),
            "--name".into(),
            "obs".into(),
            "--".into(),
            "--name".into(),
            "literal".into(),
        ])?;
        assert_eq!(name, "obs");
        assert_eq!(kept, vec!["/repo", "--", "--name", "literal"]);
        Ok(())
    }

    #[test]
    fn read_flag_reads_space_and_equals_forms() {
        let args = vec!["--model".to_string(), "opus".to_string()];
        assert_eq!(read_flag(&args, "--model"), Some("opus".to_string()));
        let args = vec!["--agent=reviewer".to_string()];
        assert_eq!(read_flag(&args, "--agent"), Some("reviewer".to_string()));
        assert_eq!(read_flag(&args, "--missing"), None);
    }
}

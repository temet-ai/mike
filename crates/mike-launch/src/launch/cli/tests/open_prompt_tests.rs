use std::io::Cursor;

use super::open_tests::open_tokens;
use super::{fake, json, project_dir, runner, toks};
use crate::config::LaunchConfig;
use crate::launch::backend::fake::FakeBackend;
use crate::launch::cli::open::{self, with_operator_args};
use crate::launch::{AssembleOptions, LaunchError, LaunchSpec, Surface, split_launch_args};

#[test]
fn with_operator_args_puts_them_between_session_id_and_the_callers() {
    let dir = tempfile::tempdir().unwrap();
    let split = split_launch_args(toks(&["--tab", "--model", "opus"])).unwrap();
    let split = with_operator_args(
        split,
        &toks(&["--dangerously-skip-permissions", "--chrome"]),
    );
    let opts = AssembleOptions {
        program: "claude".into(),
        proxy: None,
        brief_inline_max: 800,
        briefs_dir: dir.path(),
        brief_notes: false,
        brief_by_mail: false,
    };
    let spec = LaunchSpec::assemble(split, Some("brief".into()), &opts).unwrap();
    let id = spec.session.unwrap().to_string();
    assert_eq!(
        spec.args,
        toks(&[
            "--session-id",
            &id,
            "--dangerously-skip-permissions",
            "--chrome",
            "--model",
            "opus",
            "--remote-control",
            "--dangerously-load-development-channels",
            "server:mike",
            "brief"
        ])
    );
}

#[tokio::test]
async fn open_with_prompt_file_delivers_and_confirms() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let brief = home.path().join("brief.md");
    std::fs::write(&brief, "do the thing for {{session_id}}\n").unwrap();
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let tokens = open_tokens(&project, &["--prompt-file", &brief.to_string_lossy()]);
    let output = open::execute(&runner, tokens, &mut Cursor::new(""))
        .await
        .unwrap();
    assert_eq!(output.delivered, Some(true));
    assert!(output.confirmed);
    assert_eq!(output.exit_code(), 0);
    assert_eq!(json(&output.to_json_line())["brief_mode"], "inline");
}

#[tokio::test]
async fn open_with_prompt_hands_the_text_to_the_backend_not_the_argv() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let brief = home.path().join("brief.md");
    std::fs::write(&brief, "do the thing\n").unwrap();
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let tokens = open_tokens(&project, &["--prompt-file", &brief.to_string_lossy()]);
    let output = open::execute(&runner, tokens, &mut Cursor::new(""))
        .await
        .unwrap();
    assert_eq!(output.delivered, Some(true));
    assert!(output.confirmed);
    assert_eq!(output.exit_code(), 0);
    let requests = backend.open_requests.lock().await.clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].prompt.as_deref(), Some("do the thing"));
    assert_eq!(requests[0].prompt_wait_ms, 30_000);
    assert!(
        !requests[0].args.iter().any(|a| a == "do the thing"),
        "{:?}",
        requests[0].args
    );
    assert!(requests[0].args.iter().any(|a| a == "--remote-control"));
    assert_eq!(
        requests[0].args[requests[0].args.len() - 2..],
        ["--dangerously-load-development-channels", "server:mike"]
    );
}

#[tokio::test]
async fn open_without_prompt_hands_no_prompt_to_the_backend() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let output = open::execute(&runner, open_tokens(&project, &[]), &mut Cursor::new(""))
        .await
        .unwrap();
    assert_eq!(output.delivered, None);
    assert!(!output.confirmed);
    let requests = backend.open_requests.lock().await.clone();
    assert_eq!(requests[0].prompt, None);
}

fn prompt_surface(delivered: bool, detail: &str) -> Surface {
    Surface {
        delivered: Some(delivered),
        confirmed: false,
        prompt_detail: Some(detail.into()),
        ..FakeBackend::surface(None)
    }
}

#[tokio::test]
async fn open_prompt_stalled_is_exit_two_with_delivered_true_and_confirmed_false() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let brief = home.path().join("brief.md");
    std::fs::write(&brief, "do the thing\n").unwrap();
    let backend = fake();
    backend
        .open_answers
        .lock()
        .await
        .push_back(Ok(prompt_surface(
            true,
            "agent_prompt_stalled: no state change",
        )));
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let tokens = open_tokens(
        &project,
        &["--json", "--prompt-file", &brief.to_string_lossy()],
    );
    let result = open::execute(&runner, tokens, &mut Cursor::new("")).await;
    assert_eq!(open::exit_code(&result), 2);
    let failure = result.unwrap_err();
    assert!(
        matches!(failure.error, LaunchError::Unconfirmed(_)),
        "{}",
        failure.error
    );
    assert!(failure.error.to_string().contains("agent_prompt_stalled"));
    let value = json(&failure.output.unwrap().to_json_line());
    assert_eq!(value["opened"], true);
    assert_eq!(value["delivered"], true);
    assert_eq!(value["confirmed"], false);
    assert_eq!(value["reason"], "prompt_unconfirmed");
    assert_eq!(value["surface"], "w1:p1");
}

#[tokio::test]
async fn open_prompt_blocked_before_send_is_exit_two_with_delivered_false() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let brief = home.path().join("brief.md");
    std::fs::write(&brief, "do the thing\n").unwrap();
    let backend = fake();
    backend
        .open_answers
        .lock()
        .await
        .push_back(Ok(prompt_surface(false, "agent_blocked: dialog open")));
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let tokens = open_tokens(&project, &["--prompt-file", &brief.to_string_lossy()]);
    let result = open::execute(&runner, tokens, &mut Cursor::new("")).await;
    assert_eq!(open::exit_code(&result), 2);
    let failure = result.unwrap_err();
    assert!(failure.error.to_string().contains("not submitted"));
    let output = failure.output.unwrap();
    assert_eq!(output.delivered, Some(false));
    assert!(!output.confirmed);
    assert_eq!(output.reason.as_deref(), Some("prompt_undelivered"));
}

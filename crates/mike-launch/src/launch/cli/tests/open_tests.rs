use std::io::Cursor;

use uuid::Uuid;

use super::{assert_key_order, fake, json, project_dir, runner, toks};
use crate::config::LaunchConfig;
use crate::launch::backend::fake::FakeCall;
use crate::launch::cli::open;
use crate::launch::{BackendId, ChannelCheck, LaunchError, Surface, backend_for};

pub use crate::launch::cli::test_kit::open_tokens;

#[tokio::test]
async fn open_success_prints_the_contract_object_and_exits_zero() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let result = open::execute(
        &runner,
        open_tokens(&project, &["--json", "--tab"]),
        &mut Cursor::new(""),
    )
    .await;
    assert_eq!(open::exit_code(&result), 0);
    let output = result.unwrap();
    let line = output.to_json_line();
    let value = json(&line);
    assert_key_order(
        &line,
        &[
            "backend",
            "workspace",
            "folder",
            "surface",
            "session",
            "reused",
            "adopted",
            "opened",
            "delivered",
            "confirmed",
            "channel_check",
            "brief_mode",
            "run",
            "seat",
            "reason",
        ],
    );
    assert_eq!(value["backend"], "herdr");
    assert_eq!(value["workspace"], "w1");
    assert_eq!(value["surface"], "w1:p1");
    assert_eq!(value["folder"], project.to_string_lossy().as_ref());
    assert_eq!(value["opened"], true);
    assert_eq!(value["delivered"], serde_json::Value::Null);
    assert_eq!(value["confirmed"], false);
    assert_eq!(value["channel_check"], "db_unreachable");
    assert_eq!(value["brief_mode"], serde_json::Value::Null);
    assert_eq!(value["reason"], serde_json::Value::Null);
    let session: Uuid = value["session"].as_str().unwrap().parse().unwrap();
    assert_eq!(output.session, Some(session));
    let calls = backend.calls.lock().await.clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], FakeCall::List);
    let FakeCall::Open(name) = &calls[1] else {
        panic!("second call must be open: {calls:?}");
    };
    assert_eq!(name, &format!("cc-{}", &session.simple().to_string()[..8]));
    assert!(
        std::fs::read_to_string(home.path().join(".claude.json"))
            .unwrap()
            .contains("hasTrustDialogAccepted"),
        "auto_trust writes the entry before launch"
    );
}

#[tokio::test]
async fn open_untrusted_folder_is_exit_one_with_reason() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let backend = fake();
    let config = LaunchConfig {
        auto_trust: false,
        ..LaunchConfig::trusting()
    };
    let runner = runner(&backend, &config, home.path());
    let result = open::execute(
        &runner,
        open_tokens(&project, &["--json"]),
        &mut Cursor::new(""),
    )
    .await;
    assert_eq!(open::exit_code(&result), 1);
    let failure = result.unwrap_err();
    assert!(
        matches!(failure.error, LaunchError::Refused(_)),
        "{}",
        failure.error
    );
    let value = json(&failure.output.unwrap().to_json_line());
    assert_eq!(value["opened"], false);
    assert_eq!(value["reason"], "untrusted_folder");
    assert_eq!(value["session"], serde_json::Value::Null);
    let calls = backend.calls.lock().await.clone();
    assert!(!calls.iter().any(|c| matches!(c, FakeCall::Open(_))));
}

#[tokio::test]
async fn open_no_trust_flag_refuses_even_with_auto_trust_and_trust_flag_writes() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let refused = open::execute(
        &runner,
        open_tokens(&project, &["--no-trust"]),
        &mut Cursor::new(""),
    )
    .await;
    assert_eq!(open::exit_code(&refused), 1);
    assert_eq!(
        refused.unwrap_err().output.unwrap().reason.as_deref(),
        Some("untrusted_folder")
    );

    let no_auto = LaunchConfig {
        auto_trust: false,
        ..LaunchConfig::trusting()
    };
    let runner = super::runner(&backend, &no_auto, home.path());
    let written = open::execute(
        &runner,
        open_tokens(&project, &["--trust"]),
        &mut Cursor::new(""),
    )
    .await;
    assert_eq!(open::exit_code(&written), 0);
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} in {}", dir.display());
}

#[tokio::test]
async fn open_linked_worktree_is_refused_unless_worktree_ok() {
    let home = tempfile::tempdir().unwrap();
    let main = project_dir(home.path());
    git(&main, &["init", "-q"]);
    git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let linked = home.path().join("linked");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            &linked.to_string_lossy(),
            "-b",
            "wt",
        ],
    );
    let linked = std::fs::canonicalize(&linked).unwrap();
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());

    let refused = open::execute(
        &runner,
        open_tokens(&linked, &["--json"]),
        &mut Cursor::new(""),
    )
    .await;
    assert_eq!(open::exit_code(&refused), 1);
    let failure = refused.unwrap_err();
    assert!(failure.error.to_string().contains("linked git worktree"));
    assert_eq!(
        failure.output.unwrap().reason.as_deref(),
        Some("linked_worktree")
    );

    let allowed = open::execute(
        &runner,
        open_tokens(&linked, &["--worktree-ok"]),
        &mut Cursor::new(""),
    )
    .await;
    assert_eq!(open::exit_code(&allowed), 0);
}

#[tokio::test]
async fn open_surface_cap_refuses_before_opening() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let backend = fake();
    let surfaces: Vec<Surface> = (0..2)
        .map(|_| crate::launch::backend::fake::FakeBackend::surface(Some(Uuid::new_v4())))
        .collect();
    backend.list_answers.lock().await.push_back(Ok(surfaces));
    let config = LaunchConfig {
        warn_surfaces: 1,
        max_surfaces: 2,
        ..LaunchConfig::trusting()
    };
    let runner = runner(&backend, &config, home.path());
    let result = open::execute(&runner, open_tokens(&project, &[]), &mut Cursor::new("")).await;
    assert_eq!(open::exit_code(&result), 1);
    let failure = result.unwrap_err();
    assert_eq!(
        failure.output.unwrap().reason.as_deref(),
        Some("surface_cap")
    );
    let calls = backend.calls.lock().await.clone();
    assert_eq!(calls, vec![FakeCall::List]);
}

#[tokio::test]
async fn open_none_backend_is_exit_69_with_reason() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let none = backend_for(BackendId::None).unwrap();
    let config = LaunchConfig::trusting();
    let runner = runner(none.as_ref(), &config, home.path());
    let result = open::execute(
        &runner,
        open_tokens(&project, &["--json"]),
        &mut Cursor::new(""),
    )
    .await;
    assert_eq!(open::exit_code(&result), 69);
    let failure = result.unwrap_err();
    assert!(matches!(failure.error, LaunchError::BackendNone));
    let value = json(&failure.output.unwrap().to_json_line());
    assert_eq!(value["backend"], "none");
    assert_eq!(value["reason"], "backend_none");
}

#[tokio::test]
async fn open_missing_folder_is_exit_one_no_such_directory() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let missing = home.path().join("nope");
    let result = open::execute(&runner, open_tokens(&missing, &[]), &mut Cursor::new("")).await;
    assert_eq!(open::exit_code(&result), 1);
    let failure = result.unwrap_err();
    assert!(failure.error.to_string().contains("no such directory"));
    assert_eq!(
        failure.output.unwrap().reason.as_deref(),
        Some("no_such_directory")
    );
}

#[tokio::test]
async fn open_without_a_folder_or_with_a_flag_first_is_a_usage_error() {
    let home = tempfile::tempdir().unwrap();
    let backend = fake();
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    for tokens in [
        toks(&["--json"]),
        toks(&["--model", "opus", "/tmp"]),
        toks(&["-m", "opus"]),
    ] {
        let result = open::execute(&runner, tokens.clone(), &mut Cursor::new("")).await;
        assert_eq!(open::exit_code(&result), 1, "{tokens:?}");
        let failure = result.unwrap_err();
        assert!(matches!(failure.error, LaunchError::Usage(_)), "{tokens:?}");
        assert!(failure.output.is_none());
    }
}

#[tokio::test]
async fn open_backend_failure_is_exit_one_open_failed() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let backend = fake();
    backend
        .open_answers
        .lock()
        .await
        .push_back(Err(LaunchError::Timeout {
            command: "herdr agent start".into(),
            after: std::time::Duration::from_secs(1),
        }));
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());
    let result = open::execute(&runner, open_tokens(&project, &[]), &mut Cursor::new("")).await;
    assert_eq!(open::exit_code(&result), 1);
    let failure = result.unwrap_err();
    let output = failure.output.unwrap();
    assert_eq!(output.reason.as_deref(), Some("open_failed"));
    assert!(!output.opened);
}

#[tokio::test]
async fn open_channel_check_disabled_reports_disabled_and_exits_zero() {
    let home = tempfile::tempdir().unwrap();
    let project = project_dir(home.path());
    let backend = fake();
    let config = LaunchConfig {
        channel_check_enabled: false,
        ..LaunchConfig::trusting()
    };
    let runner = runner(&backend, &config, home.path());
    let output = open::execute(&runner, open_tokens(&project, &[]), &mut Cursor::new(""))
        .await
        .unwrap();
    assert_eq!(output.channel_check, ChannelCheck::Disabled);
    assert_eq!(output.exit_code(), 0);
}

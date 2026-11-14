use std::io::Cursor;

use uuid::Uuid;

use super::list_tests::row;
use super::open_tests::open_tokens;
use super::{fake, project_dir, runner};
use crate::config::LaunchConfig;
use crate::launch::backend::fake::{FakeBackend, FakeCall};
use crate::launch::cli::proxy_ledger::ProxyLedger;
pub use crate::launch::cli::test_kit::{fake_proxy, hello_route, proxy_ledger, quiet_config};
use crate::launch::cli::{kill, list, open};
use crate::launch::{CloseOutcome, Surface};

#[tokio::test]
async fn connect_reads_the_hello_and_its_launch_settings() -> anyhow::Result<()> {
    let proxy = fake_proxy(vec![hello_route()]).await?;
    let (ledger, hello) = ProxyLedger::connect(&proxy.addr)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(ledger.addr(), proxy.addr);
    assert_eq!(hello.settings[0].0, "launch_max_surfaces");
    Ok(())
}

#[tokio::test]
async fn connect_refuses_a_proxy_without_a_database_or_no_proxy_at_all() -> anyhow::Result<()> {
    let body = r#"{"database":false,"launch_brief":false,"settings":[]}"#.to_string();
    let proxy = fake_proxy(vec![("GET", "/api/launch-ledger".into(), 200, body)]).await?;
    let err = ProxyLedger::connect(&proxy.addr).await.err();
    assert!(err.is_some_and(|e| e.contains("no database")));
    assert!(ProxyLedger::connect("127.0.0.1:1").await.is_err());
    Ok(())
}

#[tokio::test]
async fn list_with_a_proxy_ledger_reads_open_launches_of_the_run() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (open_session, closed) = (Uuid::new_v4(), Uuid::new_v4());
    let mut closed_row = row(2, closed, "/proj/closed", Some("run1"), Some("dev"));
    closed_row.closed_at = Some(closed_row.created_at);
    let open_row = row(1, open_session, "/proj/open", Some("run1"), Some("obs"));
    let rows = serde_json::to_string(&vec![closed_row, open_row])?;
    let proxy = fake_proxy(vec![
        hello_route(),
        ("GET", "/api/launches".into(), 200, rows),
    ])
    .await?;
    let backend = fake();
    backend.list_answers.lock().await.push_back(Ok(vec![
        FakeBackend::surface(Some(open_session)),
        FakeBackend::surface(Some(closed)),
    ]));
    let config = LaunchConfig::trusting();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = proxy_ledger(&proxy.addr).await?;

    let listed = list::execute(&runner, Some("run1")).await?;

    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].session, Some(open_session));
    assert_eq!(listed[0].seat.as_deref(), Some("obs"));
    let read = proxy.hit("GET", "/api/launches").await;
    assert!(read.is_some_and(|s| s.target.contains("run=run1")));
    Ok(())
}

#[tokio::test]
async fn list_without_a_ledger_is_empty_for_a_run() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let backend = fake();
    backend
        .list_answers
        .lock()
        .await
        .push_back(Ok(vec![FakeBackend::surface(Some(Uuid::new_v4()))]));
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());

    assert!(list::execute(&runner, Some("run1")).await?.is_empty());
    assert_eq!(list::execute(&runner, None).await?.len(), 0);
    Ok(())
}

pub fn hosted(session: Uuid) -> Surface {
    let mut surface = FakeBackend::surface(Some(session));
    surface.pid = Some(4242);
    surface
}

#[tokio::test]
async fn kill_with_a_proxy_ledger_closes_locally_and_records_the_close() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let mut launch = row(31, session, "/proj", Some("run1"), Some("dev"));
    launch.surface_ref = Some("w1:p1".into());
    let proxy = fake_proxy(vec![
        hello_route(),
        (
            "GET",
            format!("/api/launches/{session}"),
            200,
            serde_json::to_string(&launch)?,
        ),
        (
            "POST",
            "/api/launch-ledger/launches/31/closed".into(),
            204,
            String::new(),
        ),
    ])
    .await?;
    let backend = fake();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(hosted(session))));
    backend
        .close_answers
        .lock()
        .await
        .push_back(Ok(CloseOutcome::Closed {
            process_exited: true,
        }));
    let config = LaunchConfig::trusting();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = proxy_ledger(&proxy.addr).await?;

    let report = kill::execute(&runner, session).await?;

    assert!(report.closed, "{report:?}");
    assert_eq!(report.process_exited, None);
    let closed = proxy
        .must_hit("POST", "/api/launch-ledger/launches/31/closed")
        .await?;
    let body: serde_json::Value = serde_json::from_str(&closed.body)?;
    assert_eq!(body["reason"], "kill --session");
    assert_eq!(body["process_exited"], serde_json::Value::Null);
    Ok(())
}

#[tokio::test]
async fn kill_without_a_ledger_still_closes_the_pane() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let backend = fake();
    backend
        .find_answers
        .lock()
        .await
        .push_back(Ok(Some(hosted(session))));
    backend
        .close_answers
        .lock()
        .await
        .push_back(Ok(CloseOutcome::Closed {
            process_exited: true,
        }));
    let config = LaunchConfig::trusting();
    let runner = runner(&backend, &config, home.path());

    let report = kill::execute(&runner, session).await?;

    assert!(report.closed);
    let calls = backend.calls.lock().await.clone();
    assert!(
        calls.iter().any(|c| matches!(c, FakeCall::Close(_))),
        "{calls:?}"
    );
    Ok(())
}

#[tokio::test]
async fn open_with_a_proxy_ledger_records_the_run_and_the_launch_through_it() -> anyhow::Result<()>
{
    let home = tempfile::tempdir()?;
    let project = project_dir(home.path());
    let proxy = fake_proxy(vec![
        hello_route(),
        ("GET", "/api/launch-ledger/runs".into(), 404, String::new()),
        ("POST", "/api/launch-ledger/runs".into(), 204, String::new()),
        (
            "POST",
            "/api/launch-ledger/launches/7/".into(),
            204,
            String::new(),
        ),
        (
            "POST",
            "/api/launch-ledger/launches".into(),
            201,
            r#"{"id":7}"#.into(),
        ),
    ])
    .await?;
    let backend = fake();
    let config = quiet_config();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = proxy_ledger(&proxy.addr).await?;
    runner.inspect_addr = Some(proxy.addr.clone());

    let tokens = open_tokens(&project, &["--run", "run1", "--seat", "dev"]);
    let output = open::execute(&runner, tokens, &mut Cursor::new(""))
        .await
        .map_err(|f| anyhow::anyhow!("{}", f.error))?;

    assert!(output.opened);
    let run = proxy.hit("POST", "/api/launch-ledger/runs").await;
    assert!(run.is_some_and(|s| s.body.contains("\"run_id\":\"run1\"")));
    let insert = proxy
        .must_hit("POST", "/api/launch-ledger/launches")
        .await?;
    let new: serde_json::Value = serde_json::from_str(&insert.body)?;
    assert_eq!(new["folder"], project.to_string_lossy().as_ref());
    assert_eq!(new["seat"], "dev");
    assert!(
        proxy
            .hit("POST", "/api/launch-ledger/launches/7/opened")
            .await
            .is_some()
    );
    let outcome = proxy
        .must_hit("POST", "/api/launch-ledger/launches/7/outcome")
        .await?;
    assert!(
        outcome.body.contains("\"result\":\"success\""),
        "{}",
        outcome.body
    );
    Ok(())
}

#[tokio::test]
async fn open_without_a_ledger_or_queue_still_opens_and_reports_the_record_lost()
-> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let project = project_dir(home.path());
    let backend = fake();
    let config = quiet_config();
    let runner = runner(&backend, &config, home.path());

    let output = open::execute(
        &runner,
        open_tokens(&project, &["--run", "run1"]),
        &mut Cursor::new(""),
    )
    .await
    .map_err(|f| anyhow::anyhow!("{}", f.error))?;

    assert!(output.opened);
    assert_eq!(output.run.as_deref(), Some("run1"));
    assert!(!runner.ledger.available());
    assert!(runner.ledger.lost_records());
    Ok(())
}

#[tokio::test]
async fn open_without_mike_mcp_config_or_binary_fails_before_the_pane_opens() -> anyhow::Result<()>
{
    let home = tempfile::tempdir()?;
    let project = project_dir(home.path());
    let backend = fake();
    let config = quiet_config();
    let runner = runner(&backend, &config, home.path());
    std::fs::write(home.path().join(".claude.json"), "{}")?;

    let Err(failure) = open::execute(
        &runner,
        open_tokens(&project, &["--json"]),
        &mut Cursor::new(""),
    )
    .await
    else {
        anyhow::bail!("the launch must fail without mike's MCP server");
    };

    assert!(
        failure.error.to_string().contains("mike-mcp"),
        "{}",
        failure.error
    );
    let output = failure.output.ok_or_else(|| anyhow::anyhow!("no output"))?;
    assert_eq!(output.reason.as_deref(), Some("mike_mcp_missing"));
    let calls = backend.calls.lock().await.clone();
    assert!(
        !calls.iter().any(|c| matches!(c, FakeCall::Open(_))),
        "{calls:?}"
    );
    Ok(())
}

#[tokio::test]
async fn open_without_mike_mcp_config_passes_the_server_to_the_seat() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let project = project_dir(home.path());
    let backend = fake();
    let config = quiet_config();
    let mut runner = runner(&backend, &config, home.path());
    std::fs::write(home.path().join(".claude.json"), "{}")?;
    runner.mcp_server = Some(home.path().join("mike-mcp"));

    open::execute(&runner, open_tokens(&project, &[]), &mut Cursor::new(""))
        .await
        .map_err(|f| anyhow::anyhow!("{}", f.error))?;

    let requests = backend.open_requests.lock().await.clone();
    let args = &requests
        .first()
        .ok_or_else(|| anyhow::anyhow!("not opened"))?
        .args;
    assert!(
        args.iter()
            .any(|a| a.starts_with("--mcp-config={") && a.contains("mike-mcp")),
        "{args:?}"
    );
    assert!(
        !args.iter().any(|a| a == crate::launch::DEV_CHANNELS_FLAG),
        "{args:?}"
    );
    Ok(())
}

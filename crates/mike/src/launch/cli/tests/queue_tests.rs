use std::io::Cursor;

use uuid::Uuid;

use super::list_tests::row;
use super::open_tests::open_tokens;
use super::proxy_ledger_tests::{fake_proxy, hello_route, hosted, proxy_ledger, quiet_config};
use super::{fake, project_dir, runner};
use crate::launch::CloseOutcome;
use crate::launch::cli::ledger::Ledger;
use crate::launch::cli::queue::LocalQueue;
use crate::launch::cli::queue_replay::{self, STUCK_AFTER};
use crate::launch::cli::{kill, open};

fn queued_ledger(dir: &std::path::Path) -> Ledger {
    Ledger::none().with_queue(Some(LocalQueue::at(dir.to_path_buf())))
}

#[tokio::test]
async fn open_with_nothing_reachable_queues_the_launch_and_replays_it_through_the_proxy()
-> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let project = project_dir(home.path());
    let queue_dir = home.path().join("queue");
    let backend = fake();
    let config = quiet_config();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = queued_ledger(&queue_dir);

    let tokens = open_tokens(&project, &["--run", "run1", "--seat", "dev"]);
    let output = open::execute(&runner, tokens, &mut Cursor::new(""))
        .await
        .map_err(|f| anyhow::anyhow!("{}", f.error))?;
    assert!(output.opened);
    assert!(!runner.ledger.lost_records());
    let queue = LocalQueue::at(queue_dir.clone());
    let launches = queue.launches();
    assert_eq!(launches.len(), 1);
    assert!(launches[0].1.opened.is_some());
    assert_eq!(
        launches[0].1.outcome.as_ref().map(|o| o.result.as_str()),
        Some("success")
    );
    assert_eq!(queue.runs().len(), 1);

    let proxy = fake_proxy(vec![
        hello_route(),
        ("POST", "/api/launch-ledger/runs".into(), 204, String::new()),
        (
            "POST",
            "/api/launch-ledger/launches/9/".into(),
            204,
            String::new(),
        ),
        (
            "POST",
            "/api/launch-ledger/launches".into(),
            201,
            r#"{"id":9}"#.into(),
        ),
    ])
    .await?;
    let ledger = proxy_ledger(&proxy.addr)
        .await?
        .with_queue(Some(LocalQueue::at(queue_dir)));
    let replay = queue_replay::replay(&ledger).await;

    assert_eq!(replay.replayed, 2, "{replay:?}");
    assert!(queue.is_empty());
    let insert = proxy
        .must_hit("POST", "/api/launch-ledger/launches")
        .await?;
    assert!(insert.body.contains("\"seat\":\"dev\""), "{}", insert.body);
    proxy
        .must_hit("POST", "/api/launch-ledger/launches/9/opened")
        .await?;
    proxy
        .must_hit("POST", "/api/launch-ledger/launches/9/outcome")
        .await?;
    Ok(())
}

#[tokio::test]
async fn kill_with_nothing_reachable_queues_the_close_and_replays_it_by_session()
-> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let queue_dir = home.path().join("queue");
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
    let config = quiet_config();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = queued_ledger(&queue_dir);

    let report = kill::execute(&runner, session).await?;
    assert!(report.closed);
    let queue = LocalQueue::at(queue_dir.clone());
    assert_eq!(queue.closes().len(), 1);

    let launch = row(44, session, "/proj", None, None);
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
            "/api/launch-ledger/launches/44/closed".into(),
            204,
            String::new(),
        ),
    ])
    .await?;
    let ledger = proxy_ledger(&proxy.addr)
        .await?
        .with_queue(Some(LocalQueue::at(queue_dir)));
    let replay = queue_replay::replay(&ledger).await;

    assert_eq!(replay.replayed, 1, "{replay:?}");
    assert!(queue.is_empty());
    let closed = proxy
        .must_hit("POST", "/api/launch-ledger/launches/44/closed")
        .await?;
    assert!(closed.body.contains("kill --session"), "{}", closed.body);
    Ok(())
}

#[tokio::test]
async fn a_queued_record_the_ledger_keeps_refusing_is_reported_stuck() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let queue_dir = home.path().join("queue");
    let queue = LocalQueue::at(queue_dir.clone());
    queue
        .save_launch(&crate::store::persistence::launch::NewLaunch::new(
            "herdr", "/proj",
        ))
        .map_err(anyhow::Error::msg)?;
    let proxy = fake_proxy(vec![
        hello_route(),
        (
            "POST",
            "/api/launch-ledger/launches".into(),
            500,
            "{}".into(),
        ),
    ])
    .await?;
    let ledger = proxy_ledger(&proxy.addr)
        .await?
        .with_queue(Some(LocalQueue::at(queue_dir)));

    for round in 1..=STUCK_AFTER {
        let replay = queue_replay::replay(&ledger).await;
        assert_eq!(replay.replayed, 0);
        assert_eq!(
            replay.stuck.is_empty(),
            round < STUCK_AFTER,
            "round {round}"
        );
    }
    assert_eq!(queue.launches()[0].1.attempts, STUCK_AFTER);
    Ok(())
}

#[tokio::test]
async fn nothing_reachable_and_an_unwritable_queue_loses_the_record_visibly() -> anyhow::Result<()>
{
    let home = tempfile::tempdir()?;
    let project = project_dir(home.path());
    let backend = fake();
    let config = quiet_config();
    let mut runner = runner(&backend, &config, home.path());
    runner.ledger = queued_ledger(std::path::Path::new("/proc/nonexistent/queue"));

    open::execute(&runner, open_tokens(&project, &[]), &mut Cursor::new(""))
        .await
        .map_err(|f| anyhow::anyhow!("{}", f.error))?;

    assert!(runner.ledger.lost_records());
    Ok(())
}

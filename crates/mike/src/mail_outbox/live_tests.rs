use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use super::Stage;
use super::codex::{CodexRecipient, SeatRpc};
use super::deliver::{Outcome, deliver};
use super::scripted::{database, queued};
use super::store;
use crate::launch::codex::{
    CodexError, CodexSeat, SeatKind, SeatOptions, operator_codex_home, prepare_home,
    render_contract, seat_policy,
};

const SETTLE: Duration = Duration::from_secs(300);

struct Owned<'a>(&'a CodexSeat);

#[async_trait]
impl SeatRpc for Owned<'_> {
    fn live(&self) -> bool {
        true
    }

    async fn generation(&self) -> Option<i64> {
        Some(1)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, CodexError> {
        self.0.call(method, params).await
    }
}

fn scratch() -> anyhow::Result<(PathBuf, SeatOptions)> {
    let root = std::env::temp_dir().join(format!("mike-mail-outbox-{}", Uuid::new_v4()));
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo)?;
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()?;
    anyhow::ensure!(status.success(), "git init failed in {}", repo.display());
    let home = root.join("codex-home");
    prepare_home(&home, operator_codex_home().as_deref())?;
    let options = SeatOptions {
        program: "codex".into(),
        cwd: repo,
        codex_home: home,
        model: None,
        effort: None,
        developer_instructions: render_contract(
            SeatKind::Worker,
            "codex-mail",
            Uuid::new_v4(),
            "Answer each message briefly. When a message asks for a shell command, run it.",
        ),
        policy: seat_policy(),
        mcp: None,
    };
    Ok((root, options))
}

async fn until_delivered(
    pool: &sqlx::PgPool,
    recipient: &CodexRecipient<Owned<'_>>,
    message: Uuid,
) -> anyhow::Result<Outcome> {
    let waited = tokio::time::timeout(SETTLE, async {
        loop {
            let outcome = deliver(pool, recipient, message).await?;
            let finished = matches!(&outcome, Outcome::Delivered(s) if s.has(Stage::Completed));
            if finished {
                return anyhow::Ok(outcome);
            }
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await;
    waited.map_err(|_| anyhow::anyhow!("message {message} was not delivered and completed"))?
}

#[tokio::test]
#[ignore = "runs the installed codex CLI with the operator's login: mail to an idle and a busy seat"]
async fn mail_outbox_live_codex_seat_takes_mail_idle_by_turn_start_and_busy_by_steer()
-> anyhow::Result<()> {
    let Some((db, persist)) = database().await? else {
        anyhow::bail!("no test database: set MIKE_TEST_ADMIN_URL");
    };
    let pool = persist.pool();
    let (root, options) = scratch()?;
    let mut seat = CodexSeat::launch(options).await?;
    let thread_id = seat.thread_id().to_string();

    let idle_mail = queued(&persist, 1).await?;
    {
        let recipient = CodexRecipient {
            rpc: Owned(&seat),
            thread_id: thread_id.clone(),
            pool: None,
        };
        let first = deliver(pool, &recipient, idle_mail).await?;
        assert!(matches!(first, Outcome::Accepted { .. }), "{first:?}");
        until_delivered(pool, &recipient, idle_mail).await?;
    }
    assert_eq!(store::attempts(pool, idle_mail).await?[0].mode, "start");

    let busy_turn = seat
        .brief("Run `sleep 20` in the shell, then say finished.")
        .await?;
    while let Some(event) = seat.next_event().await {
        if event["method"] == "item/started"
            && event["params"]["item"]["type"] == "commandExecution"
        {
            break;
        }
    }
    let busy_mail = queued(&persist, 1).await?;
    let recipient = CodexRecipient {
        rpc: Owned(&seat),
        thread_id,
        pool: None,
    };
    let steered = deliver(pool, &recipient, busy_mail).await?;
    assert_eq!(
        steered,
        Outcome::Accepted {
            turn_id: busy_turn.clone()
        }
    );
    let Outcome::Delivered(sighting) = until_delivered(pool, &recipient, busy_mail).await? else {
        anyhow::bail!("busy mail not delivered");
    };
    assert_eq!(sighting.turn_id.as_deref(), Some(busy_turn.as_str()));
    let attempt = &store::attempts(pool, busy_mail).await?[0];
    assert_eq!(
        (attempt.mode.as_str(), attempt.expected_turn_id.as_deref()),
        ("steer", Some(busy_turn.as_str()))
    );
    for message in [idle_mail, busy_mail] {
        for (stage, _, evidence) in store::stages(pool, message).await? {
            println!("{message} {stage:?}: {evidence}");
        }
    }
    drop(recipient);
    seat.stop().await?;
    let _ = std::fs::remove_dir_all(root);
    db.cleanup().await
}

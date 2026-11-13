use std::collections::HashSet;

use chrono::{Duration, Utc};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{
    AuditLaunch, Effects, IDLE_MINUTES, Ledger, OneShot, close_idle, is_idle, start_after_close,
    told_key,
};

#[derive(Default)]
struct Fake {
    launched: Mutex<Vec<AuditLaunch>>,
    killed: Mutex<Vec<Uuid>>,
    closed: Mutex<Vec<i64>>,
    told: Mutex<HashSet<String>>,
    fail_launch: bool,
}

#[async_trait::async_trait]
impl Effects for Fake {
    async fn launch(&self, launch: &AuditLaunch) -> Result<(), String> {
        if self.fail_launch {
            return Err("backend down".to_string());
        }
        self.launched.lock().await.push(launch.clone());
        Ok(())
    }

    async fn kill(&self, session: Uuid) -> Result<(), String> {
        self.killed.lock().await.push(session);
        Ok(())
    }

    async fn record_closed(&self, launch_id: i64, _reason: &str) -> Result<(), String> {
        self.closed.lock().await.push(launch_id);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Ledger for Fake {
    async fn already(&self, key: &str) -> bool {
        self.told.lock().await.contains(key)
    }

    async fn mark(&self, key: &str) {
        self.told.lock().await.insert(key.to_string());
    }
}

fn shot(id: i64, status: Option<&str>, quiet_minutes: i64, brief: bool) -> OneShot {
    OneShot {
        launch_id: id,
        session: Uuid::from_u128(id as u128),
        has_brief: brief,
        last_status: status.map(str::to_string),
        last_seen: Some(Utc::now() - Duration::minutes(quiet_minutes)),
    }
}

#[tokio::test]
async fn a_closed_run_requests_one_rule_audit_launch_carrying_its_run_id() -> anyhow::Result<()> {
    let fake = Fake::default();
    let run = "fleet-fleet-open-three-a0670582";
    assert!(start_after_close(&fake, &fake, "/repo", run).await);
    assert!(
        !start_after_close(&fake, &fake, "/repo", run).await,
        "a second close tick must not start a second audit"
    );
    let launched = fake.launched.lock().await;
    let [launch] = launched.as_slice() else {
        anyhow::bail!("expected one launch, got {}", launched.len());
    };
    assert_eq!(launch.source_run, run);
    assert!(launch.run_id.ends_with(run));
    assert!(launch.prompt.contains(run));
    assert_eq!(launch.folder, "/repo");
    assert!(fake.told.lock().await.contains(&told_key(run)));
    Ok(())
}

#[tokio::test]
async fn a_failed_audit_launch_is_not_recorded_so_the_next_tick_retries() -> anyhow::Result<()> {
    let fake = Fake {
        fail_launch: true,
        ..Fake::default()
    };
    assert!(!start_after_close(&fake, &fake, "/repo", "fleet-x").await);
    assert!(fake.told.lock().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_one_shot_that_finished_its_first_turn_and_sat_idle_is_closed_and_recorded()
-> anyhow::Result<()> {
    let fake = Fake::default();
    let shots = vec![
        shot(1, Some("Complete"), IDLE_MINUTES + 1, true),
        shot(2, Some("Complete"), 1, true),
        shot(3, Some("Streaming"), IDLE_MINUTES + 30, true),
        shot(4, Some("Complete"), IDLE_MINUTES + 30, false),
        shot(5, None, IDLE_MINUTES + 30, true),
    ];
    let closed = close_idle(&fake, &shots, Utc::now()).await;
    assert_eq!(closed, vec![1]);
    assert_eq!(*fake.killed.lock().await, vec![Uuid::from_u128(1)]);
    assert_eq!(*fake.closed.lock().await, vec![1]);
    assert!(is_idle(&shots[0], Utc::now()));
    assert!(!is_idle(&shots[1], Utc::now()));
    Ok(())
}

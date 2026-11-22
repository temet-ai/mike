use std::time::{Duration, Instant};

use sqlx::postgres::PgPoolOptions;

use super::DrainGate;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::pool_classes::PoolClasses;

fn handle() -> anyhow::Result<PersistHandle> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/drain_gate_fixture")?;
    Ok(PersistHandle::from_pools(PoolClasses::single(pool)))
}

#[tokio::test]
async fn the_drain_runs_when_an_embed_is_owed_or_the_backstop_is_due() -> anyhow::Result<()> {
    let persist = handle()?;
    let mut gate = DrainGate::default();
    let start = Instant::now();
    assert!(gate.should_drain(&persist, start), "the first tick looks");
    let soon = start + Duration::from_secs(30);
    assert!(!gate.should_drain(&persist, soon), "nothing owed: skip");
    persist.note_turn_embed_owed();
    assert!(
        gate.should_drain(&persist, soon),
        "an owed embed drains at once"
    );
    let later = soon + Duration::from_secs(30);
    assert!(!gate.should_drain(&persist, later), "the flag was taken");
    let backstop = soon + Duration::from_secs(600);
    assert!(
        gate.should_drain(&persist, backstop),
        "the backstop still looks"
    );
    Ok(())
}

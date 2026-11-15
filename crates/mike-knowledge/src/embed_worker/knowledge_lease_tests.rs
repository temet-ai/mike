use std::time::Instant;

use uuid::Uuid;

use super::knowledge::sweep_if_leader;
use super::knowledge_key::LEASE_ROLE;
use super::knowledge_tests::{breaker, embedding_server};
use crate::store::persistence::{PersistHandle, test_support::acquire_ephemeral_db};

#[tokio::test]
async fn a_machine_whose_embedder_fails_leaves_the_lease_to_one_that_works() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let (_server, service) = embedding_server().await;
    let failing = PersistHandle::connect(&db.database_url)
        .await?
        .with_llm_machine(Uuid::new_v4());
    let working = PersistHandle::connect(&db.database_url)
        .await?
        .with_llm_machine(Uuid::new_v4());
    let mut open = breaker();
    for _ in 0..3 {
        open.record(false, Instant::now());
    }
    assert!(!sweep_if_leader(&failing, &service, &mut open).await?);
    assert!(sweep_if_leader(&working, &service, &mut breaker()).await?);
    assert!(
        !failing
            .holds_worker_role(LEASE_ROLE, std::time::Duration::from_secs(30))
            .await
    );
    db.cleanup().await
}

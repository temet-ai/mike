use super::support::floors_in_force;
use crate::turn_facts::fixture;
use mike::store::persistence::knowledge_repo::KnowledgePersist as _;

#[tokio::test]
async fn semantic_floors_last_computed_at_is_none_until_a_floor_is_written() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let key = format!("floors-last-computed-{}", uuid::Uuid::new_v4());
    assert!(
        f.handle
            .knowledge()
            .semantic_floors_last_computed_at(&key)
            .await?
            .is_none()
    );

    floors_in_force(&f, &key, 0.8).await?;
    let last = f
        .handle
        .knowledge()
        .semantic_floors_last_computed_at(&key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("a floor was written for the key"))?;
    assert!(chrono::Utc::now() - last < chrono::Duration::minutes(5));
    assert!(
        f.handle
            .knowledge()
            .semantic_floors_last_computed_at("another-key")
            .await?
            .is_none(),
        "the newest floor is read for one embedder key only"
    );
    Ok(())
}

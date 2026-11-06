use mike::store::persistence::session_flow::{
    FlowAvailability, FlowComponent, FlowReadError, build, read_component,
};

use super::support::Fixture;

#[tokio::test]
async fn embeddings_never_reaches_the_keyset_component_dispatch() -> anyhow::Result<()> {
    let f = Fixture::new().await?;
    let mut tx = f.handle.pool().begin().await?;
    let mut page = build::empty_page(
        f.session,
        FlowComponent::Embeddings,
        FlowAvailability::Eligible {
            capture_version: 1,
            admitted_at: chrono::Utc::now(),
            producer: "test".into(),
        },
    );
    let result = read_component(&mut tx, &mut page, &[]).await;
    assert!(matches!(result, Err(FlowReadError::UnsupportedComponent)));
    tx.rollback().await?;
    f.cleanup().await
}

use mike::store::persistence::runtime_repo::RuntimePersist as _;
use serde_json::json;
use uuid::Uuid;

use super::RawEventPage;
use super::test_support::{captured, database, delta, input, page, receipt_id};

#[tokio::test]
async fn unknown_context_version_blocks_projection_without_mutating_raw() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let request = input(namespace.id, delta("A"))?;
    let receipt = captured(
        handle
            .runtime()
            .capture_codex_receipt(request.clone())
            .await?,
    )?;
    let generation = handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, Uuid::new_v4())
        .await?;
    let mut context = serde_json::to_value(&request.context)?;
    context["admission_policy_version"] = json!(999);
    sqlx::query("UPDATE runtime.codex_receipt_context SET context_document = $2::text::json WHERE event_id = $1")
        .bind(receipt.event_id).bind(serde_json::to_string(&context)?).execute(handle.pool()).await?;
    assert!(
        handle
            .runtime()
            .runtime_receipt(receipt_id(&request))
            .await
            .is_err()
    );
    assert!(
        handle
            .runtime()
            .apply_next_codex_projection(generation.generation_id)
            .await
            .is_err()
    );
    assert_eq!(
        handle
            .runtime()
            .runtime_observation(namespace.id, request.envelope.ingestion_id)
            .await?,
        Some(receipt.observation)
    );
    assert_eq!(
        super::test_support::count(&handle, namespace.id, "applications").await?,
        0
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_pages_missing_generations_and_wrong_namespaces_return_errors() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    assert!(
        handle
            .runtime()
            .begin_codex_projection_generation(namespace.id, Uuid::nil())
            .await
            .is_err()
    );
    assert!(
        handle
            .runtime()
            .begin_codex_projection_generation(Uuid::new_v4(), generation_id)
            .await
            .is_err()
    );
    assert!(
        handle
            .runtime()
            .apply_next_codex_projection(generation_id)
            .await
            .is_err()
    );
    assert!(
        handle
            .runtime()
            .activate_codex_projection_generation(generation_id)
            .await
            .is_err()
    );
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    let second = super::test_support::namespace();
    handle.runtime().register_runtime_namespace(&second).await?;
    assert!(
        handle
            .runtime()
            .begin_codex_projection_generation(second.id, generation_id)
            .await
            .is_err()
    );
    assert!(
        handle
            .runtime()
            .list_runtime_raw_events(RawEventPage {
                namespace_id: namespace.id,
                after_event_id: -1,
                limit: 1
            })
            .await
            .is_err()
    );
    assert!(
        handle
            .runtime()
            .list_runtime_raw_events(RawEventPage {
                namespace_id: namespace.id,
                after_event_id: 0,
                limit: 0
            })
            .await
            .is_err()
    );
    let mut invalid = page(generation_id);
    invalid.kind = Some("invented".into());
    assert!(
        handle
            .runtime()
            .list_codex_projection_effects(invalid)
            .await
            .is_err()
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

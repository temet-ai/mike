use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use mike::runtime::RuntimeObservation;
use mike::runtime::codex::{IntakeContext, ProjectionEffect, decode_observation};
use mike::store::persistence::{PersistError, PersistHandle};

use super::test_support::{delta, input, key, page, receipt_id};
use super::{ClassificationProvenance, CodexIntakeContext, CodexItemPage, RawEventPage};
use mike::store::persistence::runtime::codex::test_seams::{
    handle_from_pool, parse_status, validate_batch, validate_context, validate_page,
};

#[test]
fn intake_context_invalid_identity_version_and_unknown_fields_fail() -> anyhow::Result<()> {
    assert!(
        CodexIntakeContext::new(
            IntakeContext::ordinary(),
            Uuid::nil(),
            0,
            ClassificationProvenance::ReviewedOrdinaryRoute
        )
        .is_err()
    );
    assert!(
        CodexIntakeContext::new(
            IntakeContext::ordinary(),
            Uuid::new_v4(),
            -1,
            ClassificationProvenance::ReviewedOrdinaryRoute
        )
        .is_err()
    );
    let original = input(Uuid::new_v4(), delta("A"))?.context;
    for field in ["context_version", "admission_policy_version"] {
        let mut document = serde_json::to_value(&original)?;
        document[field] = json!(999);
        let decoded: CodexIntakeContext = serde_json::from_value(document)?;
        assert!(validate_context(&decoded).is_err());
    }
    let mut document = serde_json::to_value(&original)?;
    document["provider_trust"] = json!(true);
    assert!(serde_json::from_value::<CodexIntakeContext>(document).is_err());
    Ok(())
}

#[test]
fn forged_effect_receipts_identifiers_and_item_namespaces_fail_validation() -> anyhow::Result<()> {
    let request = input(Uuid::new_v4(), delta("A"))?;
    let observation = RuntimeObservation::new(request.envelope, request.raw_payload)?;
    let batch = decode_observation(&observation, IntakeContext::ordinary());
    let receipt = super::StoredRuntimeReceipt {
        event_id: 1,
        observation,
        capture_sequence: Some(1),
        context: Some(request.context),
        transport_gap: Some(false),
    };
    validate_batch(&batch, &receipt)?;
    let mut changed = batch.clone();
    changed.receipt.ingestion_id = Uuid::new_v4();
    assert!(validate_batch(&changed, &receipt).is_err());
    for mutation in 0..4 {
        let mut changed = batch.clone();
        let ProjectionEffect::Item(item) = &mut changed.effects[0] else {
            anyhow::bail!("missing item")
        };
        match mutation {
            0 => item.evidence.receipt.ingestion_id = Uuid::new_v4(),
            1 => item.key.namespace_id = Uuid::new_v4(),
            2 => item.key.thread_id = "bad\0identifier".into(),
            _ => item.evidence.json_pointer = "not-a-pointer".into(),
        }
        assert!(validate_batch(&changed, &receipt).is_err());
    }
    assert!(parse_status("invented").is_err());
    for limit in [-1, 0, 1001, i64::MAX] {
        assert!(validate_page(limit).is_err());
    }
    Ok(())
}

#[tokio::test]
async fn closed_pool_codex_operations_fail_but_auth_exclusion_needs_no_database()
-> anyhow::Result<()> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/codex_offline_fixture")?;
    pool.close().await;
    let handle = handle_from_pool(pool);
    let namespace = Uuid::new_v4();
    let generation = Uuid::new_v4();
    let request = input(namespace, delta("A"))?;
    assert!(matches!(
        handle.capture_codex_receipt(request.clone()).await,
        Err(PersistError::Database(_))
    ));
    let auth = input(
        namespace,
        json!({"method":"account/login/start", "params":{"token":"fixture"}}),
    )?;
    assert!(matches!(
        handle.capture_codex_receipt(auth).await?,
        super::CodexCaptureOutcome::Excluded(_)
    ));
    assert!(handle.runtime_receipt(receipt_id(&request)).await.is_err());
    assert!(
        handle
            .list_runtime_raw_events(RawEventPage {
                namespace_id: namespace,
                after_event_id: 0,
                limit: 1
            })
            .await
            .is_err()
    );
    assert!(
        handle
            .list_codex_unclassified_receipts(RawEventPage {
                namespace_id: namespace,
                after_event_id: 0,
                limit: 1
            })
            .await
            .is_err()
    );
    closed_projection_mutations_fail(&handle, namespace, generation).await;
    closed_projection_reads_fail(&handle, namespace, generation).await;
    closed_projection_journal_reads_fail(&handle, generation).await;
    Ok(())
}

async fn closed_projection_mutations_fail(
    handle: &PersistHandle,
    namespace: Uuid,
    generation: Uuid,
) {
    assert!(
        handle
            .begin_codex_projection_generation(namespace, generation)
            .await
            .is_err()
    );
    assert!(
        handle
            .activate_codex_projection_generation(generation)
            .await
            .is_err()
    );
    assert!(
        handle
            .apply_next_codex_projection(generation)
            .await
            .is_err()
    );
}

async fn closed_projection_reads_fail(handle: &PersistHandle, namespace: Uuid, generation: Uuid) {
    assert!(
        handle
            .codex_projection_generation(generation)
            .await
            .is_err()
    );
    assert!(
        handle
            .active_codex_projection_generation(namespace)
            .await
            .is_err()
    );
    assert!(
        handle
            .codex_projected_item(generation, &key(namespace))
            .await
            .is_err()
    );
    assert!(
        handle
            .list_codex_items(CodexItemPage {
                generation_id: generation,
                thread_id: "thread".into(),
                after: None,
                limit: 1
            })
            .await
            .is_err()
    );
    assert!(
        handle
            .codex_projection_application(generation, 1)
            .await
            .is_err()
    );
}

async fn closed_projection_journal_reads_fail(handle: &PersistHandle, generation: Uuid) {
    assert!(
        handle
            .list_codex_projection_effects(page(generation))
            .await
            .is_err()
    );
    assert!(
        handle
            .list_codex_usage_observations(page(generation))
            .await
            .is_err()
    );
    assert!(
        handle
            .list_codex_lifecycle_observations(page(generation))
            .await
            .is_err()
    );
    assert!(
        handle
            .list_codex_projection_issues(generation, 0, 1)
            .await
            .is_err()
    );
}

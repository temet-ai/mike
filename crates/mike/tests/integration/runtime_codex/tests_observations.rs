use mike::store::persistence::runtime_repo::RuntimePersist as _;
use serde_json::{Value, json};
use uuid::Uuid;

use mike::runtime::codex::{
    Field, IntakeContext, IssueKind, ProjectionEffect, ProjectionStatus, RequestId, RpcScope,
    TextChannel,
};
use mike::store::persistence::PersistHandle;
use mike::store::persistence::runtime::codex::test_seams::decode_with_stored_context;

use super::test_support::{captured, database, delta, input, key, page};
use super::{
    ClassificationProvenance, CodexIntakeContext, CodexItemPage, CodexProjectionApplication,
    CodexProjectionApplyOutcome, CodexProjectionEffect, CodexReceiptInput, StoredRuntimeReceipt,
};

fn rpc_input(namespace_id: Uuid, method: &str, result: Value) -> anyhow::Result<CodexReceiptInput> {
    let scope = RpcScope {
        request_id: RequestId::Integer(7),
        method: method.into(),
        thread_id: Some("thread".into()),
        turn_id: None,
    };
    let mut receipt = input(namespace_id, json!({"id":7,"result":result}))?;
    receipt.context = CodexIntakeContext::new(
        IntakeContext::new(Some(&scope)),
        Uuid::new_v4(),
        0,
        ClassificationProvenance::ReviewedRpcResponse,
    )?;
    Ok(receipt)
}

async fn capture_and_apply(
    handle: &PersistHandle,
    generation_id: Uuid,
    input: CodexReceiptInput,
) -> anyhow::Result<(StoredRuntimeReceipt, CodexProjectionApplication)> {
    let receipt = captured(handle.runtime().capture_codex_receipt(input).await?)?;
    let CodexProjectionApplyOutcome::Applied(application) = handle
        .runtime()
        .apply_next_codex_projection(generation_id)
        .await?
    else {
        anyhow::bail!("expected an application");
    };
    assert_eq!(application.event_id, receipt.event_id);
    assert_eq!(Some(application.capture_sequence), receipt.capture_sequence);
    assert_eq!(
        handle
            .runtime()
            .codex_projection_application(generation_id, receipt.event_id)
            .await?,
        Some(application.clone())
    );
    Ok((receipt, application))
}

fn effect_inputs(namespace_id: Uuid) -> anyhow::Result<Vec<CodexReceiptInput>> {
    Ok(vec![
        input(namespace_id, delta("whole\0message"))?,
        input(
            namespace_id,
            json!({"method":"thread/started","params":{
                "thread":{"id":"thread","model":"configured\0model","unknown":"lifecycle\0data"}
            }}),
        )?,
        input(
            namespace_id,
            json!({"method":"turn/diff/updated","params":{
                "threadId":"thread","turnId":"turn","diff":"whole\0diff"
            }}),
        )?,
        input(
            namespace_id,
            json!({"method":"thread/tokenUsage/updated","params":{
                "threadId":"thread","turnId":"turn","tokenUsage":{
                    "total":{"inputTokens":0,"outputTokens":null,"totalTokens":"unknown\0value"},
                    "last":null,"unknown":"usage\0data"
                }
            }}),
        )?,
        rpc_input(
            namespace_id,
            "account/usage/read",
            json!({"threadUsage":{
                "threadId":"thread","estimatedUsageCreditsMicros":900,
                "estimatedUsageUsdMicros":null,"groups":[{"unknown":"billing\0data"}]
            }}),
        )?,
    ])
}

async fn assert_effect_reads(
    handle: &PersistHandle,
    generation_id: Uuid,
    effects: &[CodexProjectionEffect],
) -> anyhow::Result<()> {
    for (index, kind) in ["item", "lifecycle", "signal", "usage", "billing"]
        .iter()
        .enumerate()
    {
        let mut filtered = page(generation_id);
        filtered.kind = Some((*kind).into());
        assert_eq!(
            handle
                .runtime()
                .list_codex_projection_effects(filtered)
                .await?,
            vec![effects[index].clone()]
        );
    }
    assert_eq!(
        handle
            .runtime()
            .list_codex_lifecycle_observations(page(generation_id))
            .await?,
        vec![effects[1].clone()]
    );
    assert_eq!(
        handle
            .runtime()
            .list_codex_usage_observations(page(generation_id))
            .await?,
        vec![effects[3].clone()]
    );
    let mut first_page = page(generation_id);
    first_page.limit = 2;
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_effects(first_page)
            .await?,
        effects[..2]
    );
    let mut remaining = page(generation_id);
    remaining.after = Some((effects[1].capture_sequence, effects[1].effect_ordinal));
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_effects(remaining)
            .await?,
        effects[2..]
    );
    Ok(())
}

#[tokio::test]
async fn all_effect_variants_roundtrip_nul_bytes_without_synthetic_requests() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    let mut expected = Vec::new();
    for input in effect_inputs(namespace.id)? {
        let (receipt, application) = capture_and_apply(&handle, generation_id, input).await?;
        let context = receipt
            .context
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing context"))?;
        let decoded = decode_with_stored_context(&receipt.observation, context);
        assert_eq!(application.status, ProjectionStatus::Projected);
        assert_eq!(application.effect_count, 1);
        expected.extend(decoded.effects);
    }
    let effects = handle
        .runtime()
        .list_codex_projection_effects(page(generation_id))
        .await?;
    assert_eq!(
        effects
            .iter()
            .map(|row| row.effect.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        effects
            .iter()
            .map(|row| row.capture_sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert!(effects.iter().all(|row| row.effect_ordinal == 0));
    assert_effect_reads(&handle, generation_id, &effects).await?;
    let item = handle
        .runtime()
        .codex_projected_item(generation_id, &key(namespace.id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert_eq!(
        item.item.display_text.get(&TextChannel::Message),
        Some(&"whole\0message".into())
    );
    let requests: i64 = sqlx::query_scalar("SELECT count(*) FROM requests.requests")
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(requests, 0);
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn usage_fields_preserve_missing_null_invalid_and_zero_without_summing() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    let payloads = [
        json!({}),
        json!({"total":null,"last":null,"modelContextWindow":null}),
        json!({"total":"invalid\0total","last":false,"modelContextWindow":"invalid\0window"}),
        json!({"total":{"inputTokens":0,"outputTokens":null,"totalTokens":"invalid\0tokens"},
            "last":{"totalTokens":0},"modelContextWindow":0}),
    ];
    for token_usage in &payloads {
        let event = input(
            namespace.id,
            json!({"method":"thread/tokenUsage/updated","params":{
                "threadId":"thread","turnId":"turn","tokenUsage":token_usage
            }}),
        )?;
        capture_and_apply(&handle, generation_id, event).await?;
    }
    let effects = handle
        .runtime()
        .list_codex_usage_observations(page(generation_id))
        .await?;
    assert_eq!(
        effects,
        handle
            .runtime()
            .list_codex_projection_effects(page(generation_id))
            .await?
    );
    let usages = effects
        .iter()
        .map(|effect| match &effect.effect {
            ProjectionEffect::Usage(usage) => Ok(usage.as_ref()),
            _ => anyhow::bail!("expected usage"),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    assert_eq!(usages.len(), 4);
    for (usage, payload) in usages.iter().zip(&payloads) {
        assert_eq!(&usage.provider_payload, payload);
    }
    assert_eq!(usages[0].total, Field::Missing);
    assert_eq!(usages[0].last, Field::Missing);
    assert_eq!(usages[0].context_window, Field::Missing);
    assert_eq!(usages[1].total, Field::Null);
    assert_eq!(usages[1].last, Field::Null);
    assert_eq!(usages[1].context_window, Field::Null);
    assert_eq!(usages[2].total, Field::Invalid(json!("invalid\0total")));
    assert_eq!(usages[2].last, Field::Invalid(json!(false)));
    assert_eq!(
        usages[2].context_window,
        Field::Invalid(json!("invalid\0window"))
    );
    let total = usages[3]
        .total
        .value()
        .ok_or_else(|| anyhow::anyhow!("missing total"))?;
    assert_eq!(total.input_tokens, Field::Value(0));
    assert_eq!(total.output_tokens, Field::Null);
    assert_eq!(total.total_tokens, Field::Invalid(json!("invalid\0tokens")));
    assert_eq!(total.cached_input_tokens, Field::Missing);
    assert_eq!(total.cache_write_input_tokens, Field::Missing);
    assert_eq!(total.reasoning_output_tokens, Field::Missing);
    assert_eq!(usages[3].context_window, Field::Value(0));
    assert_eq!(
        usages[3].last.value().map(|last| &last.total_tokens),
        Some(&Field::Value(0))
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn partial_unsupported_and_malformed_applications_keep_queryable_issues() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    let partial = input(
        namespace.id,
        json!({"method":"item/started","params":{
            "threadId":"thread","turnId":"turn","item":{"id":"item","type":"futureShape","opaque":"whole\0data"}
        }}),
    )?;
    let unsupported = input(
        namespace.id,
        json!({"method":"future/event","params":{"opaque":"whole\0data"}}),
    )?;
    let mut malformed = input(namespace.id, json!({}))?;
    malformed.raw_payload = b"{ invalid JSON\0".to_vec();
    let cases = [
        (
            partial,
            ProjectionStatus::Partial,
            IssueKind::UnknownItemType,
            "/params/item/type",
            1,
        ),
        (
            unsupported,
            ProjectionStatus::Unsupported,
            IssueKind::UnknownMethod,
            "/method",
            0,
        ),
        (
            malformed,
            ProjectionStatus::Malformed,
            IssueKind::MalformedJson,
            "",
            0,
        ),
    ];
    let mut applications = Vec::new();
    for (event, status, issue, pointer, effect_count) in cases {
        let raw = event.raw_payload.clone();
        let (receipt, application) = capture_and_apply(&handle, generation_id, event).await?;
        assert_eq!(receipt.observation.raw_payload, raw);
        assert_eq!(application.status, status);
        assert_eq!(application.effect_count, effect_count);
        assert_eq!(application.issues.len(), 1);
        assert_eq!(application.issues[0].kind, issue);
        assert_eq!(application.issues[0].json_pointer, pointer);
        applications.push(application);
    }
    capture_and_apply(&handle, generation_id, input(namespace.id, delta("valid"))?).await?;
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_issues(generation_id, 0, 100)
            .await?,
        applications
    );
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_issues(generation_id, 1, 1)
            .await?,
        vec![applications[1].clone()]
    );
    assert!(
        handle
            .runtime()
            .list_codex_projection_issues(generation_id, 3, 100)
            .await?
            .is_empty()
    );
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_effects(page(generation_id))
            .await?
            .len(),
        2
    );
    assert!(matches!(
        handle
            .runtime()
            .apply_next_codex_projection(generation_id)
            .await?,
        CodexProjectionApplyOutcome::CaughtUp(_)
    ));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

fn history_result() -> Value {
    json!({"data":[
        {"turnId":"turn","item":{"id":"item","type":"agentMessage","text":"first\0text"}},
        {"turnId":"turn","item":{"id":"item","type":"agentMessage","text":"last\0text"}},
        {"turnId":"turn","item":{"id":"other","type":"agentMessage","text":"sibling\0text"}}
    ],"nextCursor":"next\0cursor"})
}

async fn assert_history_items(
    handle: &PersistHandle,
    generation_id: Uuid,
    namespace_id: Uuid,
) -> anyhow::Result<()> {
    let item = handle
        .runtime()
        .codex_projected_item(generation_id, &key(namespace_id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing duplicate item"))?;
    assert_eq!(item.first_capture_sequence, 1);
    assert_eq!(item.latest_capture_sequence, 1);
    assert_eq!(item.item.first_evidence.json_pointer, "/result/data/0/item");
    assert_eq!(
        item.item.latest_evidence.json_pointer,
        "/result/data/1/item"
    );
    assert_eq!(
        item.item.display_text.get(&TextChannel::Message),
        Some(&"last\0text".into())
    );
    let mut items_page = CodexItemPage {
        generation_id,
        thread_id: "thread".into(),
        after: None,
        limit: 1,
    };
    assert_eq!(
        handle
            .runtime()
            .list_codex_items(items_page.clone())
            .await?,
        vec![item]
    );
    items_page.after = Some((1, "turn".into(), "item".into()));
    let remaining = handle.runtime().list_codex_items(items_page).await?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].item.key.item_id, "other");
    assert_eq!(
        remaining[0].item.display_text.get(&TextChannel::Message),
        Some(&"sibling\0text".into())
    );
    Ok(())
}

#[path = "tests_observations_more.rs"]
mod more;

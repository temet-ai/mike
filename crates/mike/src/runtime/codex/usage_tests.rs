use serde_json::json;

use super::test_support::{notification, observation, scope};
use super::*;

#[test]
fn usage_cumulative_samples_remain_separate_without_implicit_sums() -> anyhow::Result<()> {
    let mut samples = Vec::new();
    for total in [100, 150, 150, 25] {
        let event = notification(
            "thread/tokenUsage/updated",
            json!({
                "threadId":"t","turnId":"u","tokenUsage":{
                    "total":{"totalTokens":total,"inputTokens":0},
                    "last":{"totalTokens":50},"modelContextWindow":200000
                }
            }),
        )?;
        let batch = decode_observation(&event, IntakeContext::new(None));
        let Some(ProjectionEffect::Usage(usage)) = batch.effects.into_iter().next() else {
            anyhow::bail!("missing usage");
        };
        samples.push(usage);
    }
    let reported: Vec<_> = samples
        .iter()
        .map(|sample| sample.total.value().map(|total| total.total_tokens.clone()))
        .collect();
    assert_eq!(
        reported,
        vec![
            Some(Field::Value(100)),
            Some(Field::Value(150)),
            Some(Field::Value(150)),
            Some(Field::Value(25))
        ]
    );
    assert_ne!(samples[1].evidence.receipt, samples[2].evidence.receipt);
    assert_eq!(samples[0].context_window, Field::Value(200000));
    Ok(())
}

#[test]
fn usage_absent_zero_null_and_invalid_numeric_values_stay_distinct() -> anyhow::Result<()> {
    let event = notification(
        "thread/tokenUsage/updated",
        json!({"threadId":"t","turnId":"u","tokenUsage":{
            "total":{"inputTokens":0,"outputTokens":null,"totalTokens":"unknown"},"last":null
        }}),
    )?;
    let batch = decode_observation(&event, IntakeContext::new(None));
    let Some(ProjectionEffect::Usage(usage)) = batch.effects.first() else {
        anyhow::bail!("missing usage");
    };
    let total = usage
        .total
        .value()
        .ok_or_else(|| anyhow::anyhow!("missing total"))?;
    assert_eq!(total.input_tokens, Field::Value(0));
    assert_eq!(total.cache_write_input_tokens, Field::Missing);
    assert_eq!(total.output_tokens, Field::Null);
    assert_eq!(total.total_tokens, Field::Invalid(json!("unknown")));
    assert_eq!(usage.last, Field::Null);
    assert_eq!(usage.context_window, Field::Missing);
    Ok(())
}

#[test]
fn billing_estimate_credits_are_not_dollars_and_missing_usd_is_not_zero() -> anyhow::Result<()> {
    for usd in [None, Some(json!(null)), Some(json!(0))] {
        let mut reported =
            json!({"threadId":"Thread:Case/保留 ","estimatedUsageCreditsMicros":900,"groups":[]});
        if let Some(usd) = &usd {
            reported["estimatedUsageUsdMicros"] = usd.clone();
        }
        let event = observation(
            json!({"id":"Request:Mixed/Case","result":{"threadUsage":reported,"summary":{}}}),
        )?;
        let batch = decode_observation(
            &event,
            IntakeContext::new(Some(&scope("account/usage/read"))),
        );
        let Some(ProjectionEffect::Billing(estimate)) = batch.effects.first() else {
            anyhow::bail!("missing estimate");
        };
        assert_eq!(estimate.estimated_credits_micros, Field::Value(900));
        assert_eq!(
            estimate.estimated_usd_micros,
            match usd {
                None => Field::Missing,
                Some(serde_json::Value::Null) => Field::Null,
                Some(_) => Field::Value(0),
            }
        );
    }
    Ok(())
}

#[test]
fn billing_null_or_missing_thread_estimate_remains_unavailable() -> anyhow::Result<()> {
    for result in [
        json!({"summary":{},"threadUsage":null}),
        json!({"summary":{}}),
    ] {
        let event = observation(json!({"id":"Request:Mixed/Case","result":result}))?;
        let batch = decode_observation(
            &event,
            IntakeContext::new(Some(&scope("account/usage/read"))),
        );
        let Some(ProjectionEffect::Billing(estimate)) = batch.effects.first() else {
            anyhow::bail!("missing unavailable estimate");
        };
        assert_ne!(
            estimate.availability,
            super::usage::EstimateAvailability::Reported
        );
        assert_eq!(estimate.estimated_usd_micros, Field::Missing);
        assert_eq!(estimate.estimated_credits_micros, Field::Missing);
    }
    Ok(())
}

#[test]
fn billing_unscoped_account_summary_and_wrong_thread_are_not_seat_usage() -> anyhow::Result<()> {
    let mut unscoped = scope("account/usage/read");
    unscoped.thread_id = None;
    let event =
        observation(json!({"id":"Request:Mixed/Case","result":{"summary":{"totalTokens":999}}}))?;
    assert!(
        decode_observation(&event, IntakeContext::new(Some(&unscoped)))
            .effects
            .is_empty()
    );
    let event = observation(
        json!({"id":"Request:Mixed/Case","result":{"threadUsage":{"threadId":"foreign","estimatedUsageCreditsMicros":9,"groups":[]}}}),
    )?;
    let batch = decode_observation(
        &event,
        IntakeContext::new(Some(&scope("account/usage/read"))),
    );
    assert!(batch.effects.is_empty());
    assert_eq!(batch.issues[0].kind, IssueKind::ConflictingAssociation);
    Ok(())
}

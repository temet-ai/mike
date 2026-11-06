use serde_json::json;

use super::test_support::{envelope, observation, scope};
use super::*;

#[test]
fn unclassified_malformed_or_future_traffic_is_excluded_by_default() -> anyhow::Result<()> {
    for bytes in [
        vec![0xff, b'{'],
        br#"{"method":"future/event","params":{}}"#.to_vec(),
    ] {
        assert_eq!(
            observe(envelope(), bytes.clone(), IntakeContext::default())?,
            IntakeResult::Excluded(ExclusionReason::UnclassifiedTraffic)
        );
        let event = crate::runtime::RuntimeObservation::new(envelope(), bytes)?;
        let batch = decode_observation(&event, IntakeContext::default());
        assert_eq!(batch.status, ProjectionStatus::Excluded);
        assert_eq!(batch.issues[0].kind, IssueKind::UnclassifiedTraffic);
    }
    Ok(())
}

#[test]
fn trusted_ordinary_class_retains_malformed_bytes_with_consistent_decode_context()
-> anyhow::Result<()> {
    let context = IntakeContext::ordinary();
    let bytes = vec![0xff, b'{', 0, b'\n'];
    let IntakeResult::Captured(event) = observe(envelope(), bytes.clone(), context)? else {
        anyhow::bail!("ordinary bytes excluded");
    };
    assert_eq!(event.raw_payload, bytes);
    let batch = decode_observation(&event, context);
    assert_eq!(batch.status, ProjectionStatus::Malformed);
    assert!(batch.effects.is_empty());
    Ok(())
}

#[test]
fn recognized_auth_overrides_ordinary_class_even_before_malformed_tail() -> anyhow::Result<()> {
    for bytes in [
        br#"{"method":"account/login/start","params":{"syntheticSecret":"never-retain"}}"#.to_vec(),
        br#"{"method":"attestation/generate","params":{"broken":"#.to_vec(),
    ] {
        assert_eq!(
            observe(envelope(), bytes, IntakeContext::ordinary())?,
            IntakeResult::Excluded(ExclusionReason::Authentication)
        );
    }
    let context = IntakeContext {
        rpc_scope: None,
        traffic_class: TrafficClass::Authentication,
    };
    assert_eq!(
        observe(envelope(), vec![0xff], context)?,
        IntakeResult::Excluded(ExclusionReason::Authentication)
    );
    Ok(())
}

#[test]
fn unreviewed_response_correlations_exclude_success_error_and_malformed_payloads()
-> anyhow::Result<()> {
    for method in ["config/read", "future/read"] {
        let rpc = scope(method);
        let context = IntakeContext {
            rpc_scope: Some(&rpc),
            traffic_class: TrafficClass::OrdinaryContent,
        };
        for bytes in [
            br#"{"id":"Request:Mixed/Case","result":{"bearerToken":"synthetic"}}"#.to_vec(),
            br#"{"id":"Request:Mixed/Case","error":{"message":"synthetic"}}"#.to_vec(),
            vec![0xff],
        ] {
            assert_eq!(
                observe(envelope(), bytes.clone(), context)?,
                IntakeResult::Excluded(ExclusionReason::UnreviewedResponse)
            );
            let event = crate::runtime::RuntimeObservation::new(envelope(), bytes)?;
            assert_eq!(
                decode_observation(&event, context).status,
                ProjectionStatus::Excluded
            );
        }
    }
    Ok(())
}

#[test]
fn reviewed_content_responses_require_exact_request_ids_even_if_classified_ordinary()
-> anyhow::Result<()> {
    for method in [
        "thread/read",
        "thread/items/list",
        "thread/turns/list",
        "account/usage/read",
    ] {
        let rpc = scope(method);
        let context = IntakeContext::new(Some(&rpc));
        let bytes =
            serde_json::to_vec(&json!({"id":"Request:Mixed/Case","result":{"future":true}}))?;
        assert!(matches!(
            observe(envelope(), bytes, context)?,
            IntakeResult::Captured(_)
        ));
        let ordinary = IntakeContext {
            traffic_class: TrafficClass::OrdinaryContent,
            ..context
        };
        let bytes = serde_json::to_vec(&json!({"id":"wrong","result":{}}))?;
        assert_eq!(
            observe(envelope(), bytes, ordinary)?,
            IntakeResult::Excluded(ExclusionReason::UncorrelatedResponse)
        );
    }
    Ok(())
}

#[test]
fn mixed_method_response_and_duplicate_routing_keys_cannot_bypass_admission() -> anyhow::Result<()>
{
    let rpc = scope("thread/read");
    let context = IntakeContext {
        rpc_scope: Some(&rpc),
        traffic_class: TrafficClass::OrdinaryContent,
    };
    for bytes in [
        br#"{"method":"thread/started","id":"Request:Mixed/Case","result":{},"params":{}}"#
            .to_vec(),
        br#"{"method":null,"id":"Request:Mixed/Case","error":{}}"#.to_vec(),
        br#"{"id":"wrong","id":"Request:Mixed/Case","result":{}}"#.to_vec(),
        br#"{"method":"future/event","method":"thread/started","params":{}}"#.to_vec(),
        br#"{"method":"future/event","method":"#.to_vec(),
    ] {
        assert_eq!(
            observe(envelope(), bytes.clone(), context)?,
            IntakeResult::Excluded(ExclusionReason::AmbiguousEnvelope)
        );
        let event = crate::runtime::RuntimeObservation::new(envelope(), bytes)?;
        assert_eq!(
            decode_observation(&event, context).status,
            ProjectionStatus::Malformed
        );
    }
    let bytes = br#"{"method":"account/login/start","method":"thread/started","params":{"syntheticSecret":"never-retain"}}"#.to_vec();
    assert_eq!(
        observe(envelope(), bytes, context)?,
        IntakeResult::Excluded(ExclusionReason::Authentication)
    );
    Ok(())
}

#[test]
fn known_ordinary_method_preserves_unknown_item_type_without_explicit_class() -> anyhow::Result<()>
{
    let value = json!({"method":"item/started","params":{"threadId":"t","turnId":"u","item":{"id":"i","type":"futureItem","newField":{"whole":"a\0b"}}}});
    let bytes = serde_json::to_vec(&value)?;
    let IntakeResult::Captured(event) =
        observe(envelope(), bytes.clone(), IntakeContext::default())?
    else {
        anyhow::bail!("known ordinary method excluded");
    };
    assert_eq!(event.raw_payload, bytes);
    assert_eq!(
        decode_observation(&event, IntakeContext::default()).status,
        ProjectionStatus::Partial
    );
    let event = observation(json!({"method":"future/event","params":{"whole":"payload"}}))?;
    assert_eq!(
        decode_observation(&event, IntakeContext::ordinary()).status,
        ProjectionStatus::Unsupported
    );
    Ok(())
}

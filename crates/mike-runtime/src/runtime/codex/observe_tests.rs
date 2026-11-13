use serde_json::json;

use super::test_support::{envelope, observation, scope};
use super::*;

#[test]
fn intake_malformed_utf8_keeps_all_original_bytes() -> anyhow::Result<()> {
    let bytes = vec![b'{', 0xff, 0, b'\n'];
    let IntakeResult::Captured(event) =
        observe(envelope(), bytes.clone(), IntakeContext::ordinary())?
    else {
        anyhow::bail!("ordinary malformed frame was excluded");
    };
    assert_eq!(event.raw_payload, bytes);
    assert_eq!(event.parsed_payload, None);
    assert_eq!(
        decode_observation(&event, IntakeContext::ordinary()).status,
        ProjectionStatus::Malformed
    );
    Ok(())
}

#[test]
fn intake_unknown_fields_duplicate_keys_and_nul_content_preserve_bytes() -> anyhow::Result<()> {
    let bytes = br#" {"method":"future/event","params":{"text":"a\u0000b","x":1,"x":2}}
"#
    .to_vec();
    let IntakeResult::Captured(event) =
        observe(envelope(), bytes.clone(), IntakeContext::ordinary())?
    else {
        anyhow::bail!("unknown notification was excluded");
    };
    assert_eq!(event.raw_payload, bytes);
    assert_eq!(
        decode_observation(&event, IntakeContext::ordinary()).status,
        ProjectionStatus::Unsupported
    );
    Ok(())
}

#[test]
fn intake_auth_requests_login_frames_and_correlated_responses_exclude_content() -> anyhow::Result<()>
{
    for method in [
        "account/chatgptAuthTokens/refresh",
        "account/login/start",
        "account/login/completed",
        "mcpServer/oauth/login",
        "account/gatewayOAuth/read",
    ] {
        let bytes = serde_json::to_vec(
            &json!({"id":1,"method":method,"params":{"syntheticSecret":"never-retain"}}),
        )?;
        assert_eq!(
            observe(envelope(), bytes, IntakeContext::new(None))?,
            IntakeResult::Excluded(ExclusionReason::Authentication)
        );
        let mut auth = scope(method);
        auth.request_id = RequestId::Integer(1);
        let bytes =
            serde_json::to_vec(&json!({"id":1,"result":{"syntheticSecret":"never-retain"}}))?;
        assert_eq!(
            observe(envelope(), bytes, IntakeContext::new(Some(&auth)))?,
            IntakeResult::Excluded(ExclusionReason::Authentication)
        );
        assert_eq!(
            observe(envelope(), vec![0xff], IntakeContext::new(Some(&auth)))?,
            IntakeResult::Excluded(ExclusionReason::Authentication)
        );
    }
    Ok(())
}

#[test]
fn intake_uncorrelated_response_cannot_capture_auth_payload() -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(&json!({"id":7,"result":{"syntheticSecret":"never-retain"}}))?;
    assert_eq!(
        observe(envelope(), bytes.clone(), IntakeContext::new(None))?,
        IntakeResult::Excluded(ExclusionReason::UncorrelatedResponse)
    );
    assert_eq!(
        observe(
            envelope(),
            bytes,
            IntakeContext::new(Some(&scope("thread/read")))
        )?,
        IntakeResult::Excluded(ExclusionReason::UncorrelatedResponse)
    );
    Ok(())
}

#[test]
fn intake_invalid_local_envelope_returns_contract_error() {
    let mut invalid = envelope();
    invalid.ingestion_id = uuid::Uuid::nil();
    assert!(observe(invalid, b"{}".to_vec(), IntakeContext::ordinary()).is_err());
}

#[test]
fn decoder_inconsistent_parsed_payload_rejects_projection() -> anyhow::Result<()> {
    let mut event = observation(json!({"method":"future/event","params":{}}))?;
    event.parsed_payload = Some(json!({"different":true}));
    let batch = decode_observation(&event, IntakeContext::ordinary());
    assert_eq!(batch.status, ProjectionStatus::Malformed);
    assert_eq!(batch.issues[0].kind, IssueKind::InvalidObservation);
    Ok(())
}

#[test]
fn protocol_request_ids_preserve_string_and_integer_identity() -> anyhow::Result<()> {
    assert_eq!(
        serde_json::from_value::<RequestId>(json!("0007"))?,
        RequestId::String("0007".into())
    );
    assert_eq!(
        serde_json::from_value::<RequestId>(json!(-7))?,
        RequestId::Integer(-7)
    );
    assert!(serde_json::from_value::<RequestId>(json!(7.5)).is_err());
    assert!(serde_json::from_value::<RequestId>(json!(null)).is_err());
    Ok(())
}

#[test]
fn intake_attestation_request_and_token_response_are_excluded() -> anyhow::Result<()> {
    let auth = scope("attestation/generate");
    for value in [
        json!({"id":"Request:Mixed/Case","method":"attestation/generate","params":{}}),
        json!({"id":"Request:Mixed/Case","result":{"token":"fixture-attestation-token"}}),
    ] {
        let bytes = serde_json::to_vec(&value)?;
        let intake = observe(envelope(), bytes, IntakeContext::new(Some(&auth)))?;
        assert_eq!(
            intake,
            IntakeResult::Excluded(ExclusionReason::Authentication)
        );
        assert!(!format!("{intake:?}").contains("fixture-attestation-token"));
        let event = observation(value)?;
        let batch = decode_observation(&event, IntakeContext::new(Some(&auth)));
        assert_eq!(batch.status, ProjectionStatus::Excluded);
        assert!(batch.effects.is_empty());
        assert!(!format!("{batch:?}").contains("fixture-attestation-token"));
    }
    Ok(())
}

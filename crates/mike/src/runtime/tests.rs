use std::collections::BTreeMap;

use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::*;

fn envelope() -> RuntimeEventEnvelope {
    RuntimeEventEnvelope {
        version: 1,
        ingestion_id: Uuid::new_v4(),
        namespace_id: Uuid::new_v4(),
        source: RuntimeEventSource {
            kind: "app_server_notification".into(),
            source_version: Some("fixture-v1".into()),
            adapter: "fixture".into(),
            adapter_version: "1".into(),
            runtime_version: None,
            fidelity: "original_event_bytes".into(),
            completeness: "unknown".into(),
            extra: BTreeMap::new(),
        },
        received_at: Utc::now(),
        provider_timestamp: None,
        provider_session_id: Some("shared-session@host".into()),
        provider_thread_id: Some("thread:Child/Case".into()),
        provider_turn_id: Some("turn:not-a-uuid".into()),
        provider_item_id: Some("item:MixedCase".into()),
        provider_event_id: None,
        source_cursor: None,
        extra: BTreeMap::new(),
    }
}

fn binding() -> RuntimeThreadBinding {
    RuntimeThreadBinding {
        mike_session_id: Uuid::new_v4(),
        provider: ProviderThreadRef {
            namespace_id: Uuid::new_v4(),
            thread_id: "Thread:保留/Case ".into(),
        },
        provider_session_id: Some("Shared/Session".into()),
        owning_seat_id: Some(Uuid::new_v4()),
        native_parent: None,
        forked_from: None,
    }
}

#[test]
fn selector_legacy_default_and_explicit_claude_are_compatible() -> anyhow::Result<()> {
    #[derive(Deserialize)]
    struct Config {
        #[serde(default)]
        runtime: RuntimeKind,
    }
    let legacy: Config = serde_json::from_str("{}")?;
    let explicit: Config = serde_json::from_str(r#"{"runtime":"claude"}"#)?;
    assert_eq!(legacy.runtime, explicit.runtime);
    assert_eq!(legacy.runtime.as_str(), "claude");
    legacy.runtime.ensure_managed_launch_supported()?;
    assert!(
        RuntimeKind::Codex
            .ensure_managed_launch_supported()
            .is_err()
    );
    assert!(serde_json::from_str::<Config>(r#"{"runtime":"unknown"}"#).is_err());
    Ok(())
}

#[test]
fn provider_identity_non_uuid_roundtrip_preserves_case_and_spacing() -> anyhow::Result<()> {
    let identity = binding();
    identity.validate()?;
    let decoded: RuntimeThreadBinding = serde_json::from_str(&serde_json::to_string(&identity)?)?;
    assert_eq!(decoded, identity);
    Ok(())
}

#[test]
fn native_child_parent_and_fork_are_independent() -> anyhow::Result<()> {
    let parent = binding();
    let mut child = binding();
    child.provider.namespace_id = parent.provider.namespace_id;
    child.provider.thread_id = "thread:child".into();
    child.provider_session_id = parent.provider_session_id.clone();
    child.owning_seat_id = Some(parent.mike_session_id);
    child.native_parent = Some(parent.provider.clone());
    child.forked_from = Some(ProviderThreadRef {
        namespace_id: Uuid::new_v4(),
        thread_id: "unresolved:fork".into(),
    });
    child.validate()?;
    assert_ne!(parent.mike_session_id, child.mike_session_id);
    assert_ne!(child.native_parent, child.forked_from);
    assert_eq!(child.provider_session_id, parent.provider_session_id);
    Ok(())
}

#[test]
fn ancestry_self_parent_and_self_fork_are_rejected() {
    let mut identity = binding();
    identity.native_parent = Some(identity.provider.clone());
    assert!(matches!(
        identity.validate(),
        Err(RuntimeContractError::SelfReference(_))
    ));
    identity.native_parent = None;
    identity.forked_from = Some(identity.provider.clone());
    assert!(matches!(
        identity.validate(),
        Err(RuntimeContractError::SelfReference(_))
    ));
}

#[test]
fn native_child_missing_or_self_owner_is_rejected() {
    let mut identity = binding();
    identity.native_parent = Some(binding().provider);
    identity.owning_seat_id = None;
    assert_eq!(
        identity.validate(),
        Err(RuntimeContractError::MissingOwningSeat)
    );
    identity.owning_seat_id = Some(identity.mike_session_id);
    assert_eq!(
        identity.validate(),
        Err(RuntimeContractError::ChildOwnsItself)
    );
}

#[test]
fn identity_empty_nul_and_nil_fields_are_rejected() {
    let mut identity = binding();
    identity.provider.thread_id = " \t".into();
    assert!(identity.validate().is_err());
    identity.provider.thread_id = "opaque\0id".into();
    assert!(identity.validate().is_err());
    identity.provider = binding().provider;
    identity.mike_session_id = Uuid::nil();
    assert!(identity.validate().is_err());
    let namespace = RuntimeNamespace {
        id: Uuid::new_v4(),
        runtime: RuntimeKind::Codex,
        durable_key: String::new(),
    };
    assert!(namespace.validate().is_err());
}

#[test]
fn observation_unknown_event_and_duplicate_keys_keep_original_bytes() -> anyhow::Result<()> {
    let raw = br#" { "type":"future/item", "x":1, "x":2, "nested":{"unknown":true} }
"#
    .to_vec();
    let observation = RuntimeObservation::new(envelope(), raw.clone())?;
    let decoded: RuntimeObservation = serde_json::from_slice(&serde_json::to_vec(&observation)?)?;
    assert_eq!(decoded.raw_payload, raw);
    assert_eq!(decoded, observation);
    decoded.validate()?;
    Ok(())
}

#[test]
fn observation_invalid_json_and_non_utf8_are_retained() -> anyhow::Result<()> {
    for raw in [b"{bad-json".to_vec(), vec![0xff, 0, 0xfe], Vec::new()] {
        let observation = RuntimeObservation::new(envelope(), raw.clone())?;
        assert_eq!(observation.raw_payload, raw);
        assert_eq!(observation.parsed_payload, None);
        observation.validate()?;
    }
    Ok(())
}

#[test]
fn observation_future_versions_and_unknown_metadata_roundtrip() -> anyhow::Result<()> {
    let mut metadata = envelope();
    metadata.version = 900;
    metadata.source.fidelity = "future-fidelity".into();
    metadata
        .source
        .extra
        .insert("future_source".into(), json!({"a": "\0"}));
    metadata
        .extra
        .insert("future_envelope".into(), json!([1, 2, 3]));
    let observation = RuntimeObservation::new(metadata, br#"{"value":"\u0000"}"#.to_vec())?;
    let restored: RuntimeObservation = serde_json::from_value(serde_json::to_value(&observation)?)?;
    assert_eq!(restored, observation);
    restored.validate()?;
    Ok(())
}

#[test]
fn observation_invalid_envelope_and_mismatched_projection_fail() -> anyhow::Result<()> {
    let mut metadata = envelope();
    metadata.version = 0;
    assert!(RuntimeObservation::new(metadata, Vec::new()).is_err());
    let mut observation = RuntimeObservation::new(envelope(), b"null".to_vec())?;
    assert_eq!(observation.parsed_payload, Some(serde_json::Value::Null));
    observation.parsed_payload = None;
    assert_eq!(
        observation.validate(),
        Err(RuntimeContractError::InconsistentProjection)
    );
    observation.envelope = envelope();
    observation.envelope.source.kind.clear();
    assert!(observation.envelope.validate().is_err());
    Ok(())
}

#[test]
fn envelope_extensions_cannot_replace_defined_fields() {
    let mut metadata = envelope();
    metadata.extra.insert("version".into(), json!(2));
    assert!(matches!(
        metadata.validate(),
        Err(RuntimeContractError::ConflictingExtension(_))
    ));
    metadata.extra.clear();
    metadata
        .source
        .extra
        .insert("kind".into(), json!("replacement"));
    assert!(matches!(
        metadata.validate(),
        Err(RuntimeContractError::ConflictingExtension(_))
    ));
}

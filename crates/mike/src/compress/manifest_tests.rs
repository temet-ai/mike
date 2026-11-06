use super::*;

fn target_stub(outcome: StageOutcome) -> TargetEntry {
    TargetEntry {
        outcome,
        content_kind: "read_result_text".to_string(),
        json_path: None,
        tool_use_id: None,
        producer: None,
        input_range: ByteRange::UNLOCATED,
        output_range: ByteRange::UNLOCATED,
        original_bytes: 0,
        sent_bytes: 0,
        original_tokens: None,
        sent_tokens: None,
        original_content_sha256: sha256_hex(b""),
        sent_content_sha256: sha256_hex(b""),
        recovery_ref: None,
        reason_code: "stub".to_string(),
        detail: None,
    }
}

fn stage_stub(ordinal: u32, input: &str, output: &str, outcome: StageOutcome) -> StageEntry {
    StageEntry {
        ordinal,
        stage: "compression.file_read".to_string(),
        outcome,
        policy_id: "file_read".to_string(),
        policy_version: 1,
        resolved_parameters: serde_json::json!({}),
        input_sha256: input.to_string(),
        output_sha256: output.to_string(),
        targets: vec![target_stub(outcome)],
        reason_code: "stub".to_string(),
        detail: None,
    }
}

#[test]
fn verify_chain_empty_stages_ok_when_bodies_match() {
    let m = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: sha256_hex(b"same"),
        sent_sha256: sha256_hex(b"same"),
        stages: Vec::new(),
    };
    assert_eq!(m.verify_chain(), Ok(()));
}

#[test]
fn verify_chain_empty_stages_errs_when_bodies_differ() {
    let m = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: sha256_hex(b"a"),
        sent_sha256: sha256_hex(b"b"),
        stages: Vec::new(),
    };
    assert_eq!(m.verify_chain(), Err(ChainError::EmptyStagesBodyChanged));
}

#[test]
fn verify_chain_single_stage_closes() {
    let orig = sha256_hex(b"original body");
    let sent = sha256_hex(b"sent body");
    let m = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: orig.clone(),
        sent_sha256: sent.clone(),
        stages: vec![stage_stub(1, &orig, &sent, StageOutcome::Applied)],
    };
    assert_eq!(m.verify_chain(), Ok(()));
}

#[test]
fn verify_chain_rejects_first_stage_input_mismatch() {
    let orig = sha256_hex(b"original body");
    let sent = sha256_hex(b"sent body");
    let m = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: orig,
        sent_sha256: sent.clone(),
        stages: vec![stage_stub(
            1,
            "not-the-original-sha",
            &sent,
            StageOutcome::Applied,
        )],
    };
    assert_eq!(
        m.verify_chain(),
        Err(ChainError::FirstStageInputMismatch { ordinal: 1 })
    );
}

#[test]
fn verify_chain_rejects_broken_stage_link() {
    let orig = sha256_hex(b"original body");
    let mid = sha256_hex(b"mid body");
    let sent = sha256_hex(b"sent body");
    let m = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: orig.clone(),
        sent_sha256: sent.clone(),
        stages: vec![
            stage_stub(1, &orig, &mid, StageOutcome::Applied),
            stage_stub(2, "not-mid-sha", &sent, StageOutcome::Applied),
        ],
    };
    assert_eq!(
        m.verify_chain(),
        Err(ChainError::StageChainBroken { ordinal: 2 })
    );
}

#[test]
fn verify_chain_rejects_last_stage_output_mismatch() {
    let orig = sha256_hex(b"original body");
    let m = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: orig.clone(),
        sent_sha256: sha256_hex(b"expected sent body"),
        stages: vec![stage_stub(
            1,
            &orig,
            "not-the-sent-sha",
            StageOutcome::Applied,
        )],
    };
    assert_eq!(
        m.verify_chain(),
        Err(ChainError::LastStageOutputMismatch { ordinal: 1 })
    );
}

#[test]
fn verify_chain_ok_on_a_no_op_single_stage() {
    let sha = sha256_hex(b"untouched body");
    let m = TransformationManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        original_sha256: sha.clone(),
        sent_sha256: sha.clone(),
        stages: vec![stage_stub(1, &sha, &sha, StageOutcome::Unchanged)],
    };
    assert_eq!(m.verify_chain(), Ok(()));
}

#[test]
fn sha256_hex_matches_known_vector() {
    assert_eq!(
        sha256_hex(b"hello world"),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

fn applied_stub<'a>(stage: &'a str, new_bytes: &'a [u8]) -> AppliedStage<'a> {
    AppliedStage {
        stage,
        policy_id: stage,
        policy_version: 1,
        resolved_parameters: serde_json::json!({}),
        new_bytes,
        targets: Vec::new(),
        reason_code: "stub",
        detail: None,
    }
}

#[test]
fn manifest_builder_chain_closes_across_n_stages() {
    let original = b"original body";
    let mut builder = ManifestBuilder::new(sha256_hex(original));
    builder.record_applied(applied_stub("filter", b"after filter"));
    builder.record_applied(applied_stub("reminders", b"after reminders"));
    builder.record_applied(applied_stub("composer", b"after composer"));
    let manifest = builder.finish(sha256_hex(b"after composer"));

    assert_eq!(manifest.stages.len(), 3);
    assert_eq!(manifest.verify_chain(), Ok(()));
    assert_eq!(manifest.stages[0].input_sha256, sha256_hex(original));
    assert_eq!(
        manifest.stages[1].input_sha256,
        manifest.stages[0].output_sha256
    );
    assert_eq!(
        manifest.stages[2].input_sha256,
        manifest.stages[1].output_sha256
    );
    assert_eq!(manifest.stages[2].output_sha256, manifest.sent_sha256);
}

#[test]
fn manifest_builder_record_unchanged_keeps_input_eq_output() {
    let seed = sha256_hex(b"untouched");
    let mut builder = ManifestBuilder::new(seed.clone());
    builder.record_unchanged("composer", "composer", "passthrough");
    let manifest = builder.finish(seed.clone());

    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].outcome, StageOutcome::Unchanged);
    assert_eq!(manifest.stages[0].input_sha256, seed);
    assert_eq!(manifest.stages[0].output_sha256, seed);
    assert_eq!(manifest.verify_chain(), Ok(()));
}

#[test]
fn manifest_builder_empty_stages_yields_original_eq_sent() {
    let seed = sha256_hex(b"identity request");
    let builder = ManifestBuilder::new(seed.clone());
    let manifest = builder.finish(seed.clone());

    assert!(manifest.stages.is_empty());
    assert_eq!(manifest.original_sha256, manifest.sent_sha256);
    assert_eq!(manifest.verify_chain(), Ok(()));
}

#[test]
fn record_unchanged_with_keeps_parameters_and_detail_and_the_hash_chain() {
    let seed = sha256_hex(b"request");
    let mut builder = ManifestBuilder::new(seed.clone());
    builder.record_unchanged_with(UnchangedStage {
        stage: "read_dedup",
        policy_id: "read_dedup",
        reason_code: "read_dedup_observed_disarmed",
        resolved_parameters: serde_json::json!({ "hit_count": 2 }),
        targets: Vec::new(),
        detail: Some("observed".to_string()),
    });
    let manifest = builder.finish(seed.clone());

    assert_eq!(manifest.stages.len(), 1);
    let stage = &manifest.stages[0];
    assert_eq!(stage.outcome, StageOutcome::Unchanged);
    assert_eq!(stage.reason_code, "read_dedup_observed_disarmed");
    assert_eq!(stage.resolved_parameters["hit_count"], 2);
    assert_eq!(stage.detail.as_deref(), Some("observed"));
    assert_eq!(stage.input_sha256, seed);
    assert_eq!(stage.output_sha256, seed);
    assert_eq!(manifest.verify_chain(), Ok(()));
}

#[test]
fn manifest_builder_finish_sets_sent_sha256() {
    let builder = ManifestBuilder::new(sha256_hex(b"original"));
    let manifest = builder.finish(sha256_hex(b"sent bytes"));
    assert_eq!(manifest.sent_sha256, sha256_hex(b"sent bytes"));
}

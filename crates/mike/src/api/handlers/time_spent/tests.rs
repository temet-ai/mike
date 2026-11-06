use chrono::TimeZone;

use super::*;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 12, 12, 0, 0)
        .single()
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

#[test]
fn resolve_no_params_defaults_to_last_fourteen_days_main_sessions_only() -> anyhow::Result<()> {
    let resolved =
        resolve_time_spent(&TimeSpentParams::default(), now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.to, now());
    assert_eq!(resolved.from, now() - Duration::days(14));
    assert!(!resolved.include_subagents);
    assert_eq!(resolved.limit, 20);
    assert_eq!(resolved.offset, 0);
    assert_eq!(resolved.project, None);
    assert_eq!(resolved.session, None);
    assert_eq!(resolved.kind, None);
    Ok(())
}

#[test]
fn resolve_days_sets_window_when_from_absent() -> anyhow::Result<()> {
    let params = TimeSpentParams {
        days: Some(3),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&params, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.from, now() - Duration::days(3));
    Ok(())
}

#[test]
fn resolve_days_outside_range_is_clamped() -> anyhow::Result<()> {
    let too_many = TimeSpentParams {
        days: Some(400),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&too_many, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.from, now() - Duration::days(90));
    let too_few = TimeSpentParams {
        days: Some(0),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&too_few, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.from, now() - Duration::days(1));
    Ok(())
}

#[test]
fn resolve_days_is_ignored_when_from_given() -> anyhow::Result<()> {
    let params = TimeSpentParams {
        from: Some("2026-10-01T00:00:00Z".into()),
        days: Some(3),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&params, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(
        resolved.from,
        Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0)
            .single()
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
    );
    assert_eq!(resolved.to, now());
    Ok(())
}

#[test]
fn resolve_to_given_without_from_counts_days_back_from_to() -> anyhow::Result<()> {
    let params = TimeSpentParams {
        to: Some("2026-10-10T00:00:00Z".into()),
        days: Some(2),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&params, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.to - resolved.from, Duration::days(2));
    Ok(())
}

#[test]
fn resolve_from_not_before_to_is_rejected_with_both_bounds() {
    let params = TimeSpentParams {
        from: Some("2026-10-12T12:00:00Z".into()),
        to: Some("2026-10-12T12:00:00Z".into()),
        ..TimeSpentParams::default()
    };
    let err = resolve_time_spent(&params, now()).err().unwrap_or_default();
    assert!(err.contains("must be earlier than"), "got {err:?}");
    assert!(err.contains("2026-10-12T12:00:00+00:00"), "got {err:?}");
}

#[test]
fn resolve_bad_timestamp_is_rejected_with_the_input() {
    let params = TimeSpentParams {
        from: Some("yesterday".into()),
        ..TimeSpentParams::default()
    };
    let err = resolve_time_spent(&params, now()).err().unwrap_or_default();
    assert!(
        err.contains("`from`") && err.contains("\"yesterday\""),
        "got {err:?}"
    );
}

#[test]
fn resolve_bad_session_uuid_is_rejected_with_the_input() {
    let params = TimeSpentParams {
        session_id: Some("not-a-uuid".into()),
        ..TimeSpentParams::default()
    };
    let err = resolve_time_spent(&params, now()).err().unwrap_or_default();
    assert!(
        err.contains("`session_id`") && err.contains("\"not-a-uuid\""),
        "got {err:?}"
    );
}

#[test]
fn resolve_valid_session_uuid_and_filters_are_kept() -> anyhow::Result<()> {
    let params = TimeSpentParams {
        project: Some(" temet-proxy ".into()),
        session_id: Some("5b0ef8f3-2c3d-4b7a-9f0e-6f1a2b3c4d5e".into()),
        kind: Some("rust build".into()),
        include_subagents: true,
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&params, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.project.as_deref(), Some("temet-proxy"));
    assert_eq!(
        resolved.session,
        Some(Uuid::parse_str("5b0ef8f3-2c3d-4b7a-9f0e-6f1a2b3c4d5e")?)
    );
    assert_eq!(resolved.kind.as_deref(), Some("rust build"));
    assert!(resolved.include_subagents);
    Ok(())
}

#[test]
fn resolve_blank_strings_count_as_absent() -> anyhow::Result<()> {
    let params = TimeSpentParams {
        project: Some("  ".into()),
        session_id: Some(String::new()),
        kind: Some(String::new()),
        from: Some(String::new()),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&params, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.project, None);
    assert_eq!(resolved.session, None);
    assert_eq!(resolved.kind, None);
    assert_eq!(resolved.from, now() - Duration::days(14));
    Ok(())
}

#[test]
fn resolve_limit_is_clamped_between_one_and_one_hundred() -> anyhow::Result<()> {
    for (given, expected) in [
        (Some(0), 1),
        (Some(-5), 1),
        (Some(50), 50),
        (Some(5000), 100),
        (None, 20),
    ] {
        let params = TimeSpentParams {
            limit: given,
            ..TimeSpentParams::default()
        };
        let resolved = resolve_time_spent(&params, now()).map_err(anyhow::Error::msg)?;
        assert_eq!(resolved.limit, expected, "limit {given:?}");
    }
    Ok(())
}

#[test]
fn resolve_offset_defaults_to_zero_and_never_goes_negative() -> anyhow::Result<()> {
    for (given, expected) in [(None, 0), (Some(-3), 0), (Some(40), 40)] {
        let params = TimeSpentParams {
            offset: given,
            ..TimeSpentParams::default()
        };
        let resolved = resolve_time_spent(&params, now()).map_err(anyhow::Error::msg)?;
        assert_eq!(resolved.offset, expected, "offset {given:?}");
    }
    Ok(())
}

#[test]
fn resolve_project_path_is_kept_exact_alongside_the_substring_project() -> anyhow::Result<()> {
    let both = TimeSpentParams {
        project: Some("proxy".into()),
        project_path: Some(" /mnt/data/projects/temet/temet-proxy ".into()),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&both, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.project.as_deref(), Some("proxy"));
    assert_eq!(
        resolved.project_path.as_deref(),
        Some("/mnt/data/projects/temet/temet-proxy")
    );
    let neither =
        resolve_time_spent(&TimeSpentParams::default(), now()).map_err(anyhow::Error::msg)?;
    assert_eq!(neither.project_path, None);
    let blank = TimeSpentParams {
        project_path: Some("  ".into()),
        ..TimeSpentParams::default()
    };
    let resolved = resolve_time_spent(&blank, now()).map_err(anyhow::Error::msg)?;
    assert_eq!(resolved.project_path, None);
    Ok(())
}

fn by_kind_row(kind: &str, answers: i64, wait: f64, model: f64) -> TimeSpentByKindRow {
    TimeSpentByKindRow {
        kind: kind.into(),
        answers,
        answers_over_an_hour: 0,
        wait_seconds: wait,
        p50_wait_seconds: 1.0,
        p90_wait_seconds: 2.0,
        model_seconds: model,
        sessions: 1,
    }
}

#[test]
fn response_totals_sum_over_by_kind_and_json_shape_is_stable() -> anyhow::Result<()> {
    let resolved =
        resolve_time_spent(&TimeSpentParams::default(), now()).map_err(anyhow::Error::msg)?;
    let response = build_time_spent_response(
        resolved,
        vec![
            by_kind_row("gate", 2, 100.5, 10.0),
            by_kind_row("git", 3, 20.0, 5.5),
        ],
        vec![SlowCommandRow {
            command: "scripts/gate".into(),
            kind: "gate".into(),
            calls: 2,
            seconds: 100.5,
            p50_seconds: 50.0,
            sessions: 1,
        }],
        vec![BuildsRunningAtOnceRow {
            running_at_once: "none".into(),
            sort_key: 0,
            calls: 4,
            seconds: 40.0,
            p50_seconds: 10.0,
        }],
    );
    let value = serde_json::to_value(&response)?;
    let keys: Vec<&str> = value
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    for key in [
        "from",
        "to",
        "project",
        "project_path",
        "include_subagents",
        "session_id",
        "totals",
        "by_kind",
        "slow_commands",
        "builds_running_at_once",
        "notes",
    ] {
        assert!(keys.contains(&key), "missing {key} in {keys:?}");
    }
    assert_eq!(value["totals"]["wait_seconds"], 120.5);
    assert_eq!(value["totals"]["model_seconds"], 15.5);
    assert_eq!(value["totals"]["answers"], 5);
    assert_eq!(value["by_kind"][0]["kind"], "gate");
    assert_eq!(value["slow_commands"][0]["command"], "scripts/gate");
    assert_eq!(
        value["builds_running_at_once"][0]["running_at_once"],
        "none"
    );
    assert_eq!(value["notes"].as_array().map(Vec::len), Some(5));
    assert_eq!(value["session_id"], serde_json::Value::Null);
    Ok(())
}

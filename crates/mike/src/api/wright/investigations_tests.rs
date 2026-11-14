use super::investigations::{LimitsBody, validate_limits};

#[test]
fn engineer_limits_patch_out_of_range_names_field_value_and_range() -> anyhow::Result<()> {
    let body = LimitsBody {
        wall_seconds: Some(5),
        ..LimitsBody::default()
    };
    let Err(reason) = validate_limits(&body) else {
        anyhow::bail!("5 wall seconds was accepted");
    };
    assert_eq!(reason, "wall_seconds is 5; it must be between 60 and 7200");
    let body = LimitsBody {
        tokens: Some(60_000_000),
        ..LimitsBody::default()
    };
    assert!(validate_limits(&body).is_err_and(|r| r.contains("tokens is 60000000")));
    Ok(())
}

#[test]
fn engineer_limits_patch_with_no_field_is_refused_and_in_range_passes() {
    assert!(validate_limits(&LimitsBody::default()).is_err_and(|r| r.contains("names no limit")));
    let body = LimitsBody {
        daily_starts: Some(0),
        tool_calls: Some(100),
        ..LimitsBody::default()
    };
    let patch = validate_limits(&body);
    assert!(patch.is_ok_and(|p| p.daily_starts == Some(0) && p.tool_calls == Some(100)));
}

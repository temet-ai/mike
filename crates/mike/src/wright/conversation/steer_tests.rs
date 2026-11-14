use chrono::{Duration, Utc};
use serde_json::json;

use super::answer::named_problems;
use super::steer::{Change, LimitField, parse};

#[test]
fn parse_out_of_set_kind_is_refused_with_the_kind_named() -> anyhow::Result<()> {
    let now = Utc::now();
    let Err(reason) = parse(&json!({"kind": "restart_proxy"}), now) else {
        anyhow::bail!("restart_proxy must be refused");
    };
    assert!(reason.contains("restart_proxy"), "{reason}");
    assert!(reason.contains("pause_remedy"), "{reason}");
    assert!(parse(&json!("pause everything"), now).is_err());
    assert!(parse(&json!({"name": "judge"}), now).is_err());
    Ok(())
}

#[test]
fn parse_extra_field_is_refused_with_the_field_named() -> anyhow::Result<()> {
    let until = (Utc::now() + Duration::hours(2)).to_rfc3339();
    let Err(reason) = parse(
        &json!({"kind": "pause_remedy", "name": "judge", "until": until, "also": "deploy"}),
        Utc::now(),
    ) else {
        anyhow::bail!("an extra field must be refused");
    };
    assert!(reason.contains("steer.also"), "{reason}");
    Ok(())
}

#[test]
fn parse_out_of_range_values_are_refused() {
    let now = Utc::now();
    let limit = parse(
        &json!({"kind": "set_engineer_limit", "field": "tokens", "value": 10}),
        now,
    );
    assert!(limit.is_err_and(|r| r.contains("between 10000 and 5000000")));
    let unknown = parse(
        &json!({"kind": "set_engineer_limit", "field": "budget", "value": 10}),
        now,
    );
    assert!(unknown.is_err_and(|r| r.contains("budget")));
    let far = (now + Duration::days(30)).to_rfc3339();
    assert!(parse(&json!({"kind": "skip_problem", "id": 3, "until": far}), now).is_err());
    let past = (now - Duration::hours(1)).to_rfc3339();
    assert!(
        parse(
            &json!({"kind": "pause_remedy", "name": "judge", "until": past}),
            now
        )
        .is_err()
    );
    assert!(parse(&json!({"kind": "skip_problem", "id": 0, "until": (now + Duration::hours(1)).to_rfc3339()}), now).is_err());
}

#[test]
fn parse_in_set_in_range_changes_round_trip() -> anyhow::Result<()> {
    let now = Utc::now();
    let until = now + Duration::hours(3);
    let pause = parse(
        &json!({"kind": "pause_remedy", "name": "judge", "until": until.to_rfc3339()}),
        now,
    )
    .map_err(anyhow::Error::msg)?;
    let Change::PauseRemedy { name, .. } = &pause else {
        anyhow::bail!("expected pause_remedy, got {pause:?}");
    };
    assert_eq!(name, "judge");
    let limit = parse(
        &json!({"kind": "set_engineer_limit", "field": "daily_starts", "value": 2}),
        now,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(
        limit,
        Change::SetEngineerLimit {
            field: LimitField::DailyStarts,
            value: 2
        }
    );
    let back: Change = serde_json::from_value(pause.to_json())?;
    assert_eq!(back, pause);
    let open = parse(
        &json!({"kind": "open_problem", "check": "cold_forks", "resource": "fleet"}),
        now,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(serde_json::from_value::<Change>(open.to_json())?, open);
    Ok(())
}

#[test]
fn named_problems_reads_hash_and_word_references() {
    assert_eq!(
        named_problems("why is #12 open, and problem 7, and problem #7?"),
        vec![7, 12]
    );
    assert!(named_problems("nothing named here").is_empty());
}

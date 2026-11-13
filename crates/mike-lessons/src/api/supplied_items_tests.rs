use chrono::{Datelike, TimeZone, Utc};

use super::{figures_answer, week_start};
use crate::store::persistence::supplied_items::SupplyFigure;

#[test]
fn week_start_is_the_monday_of_the_week_given() -> anyhow::Result<()> {
    let start = week_start(Some("2026-11-12")).map_err(anyhow::Error::msg)?;
    assert_eq!(
        start,
        Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0)
            .single()
            .ok_or_else(|| anyhow::anyhow!("date"))?
    );
    assert_eq!(
        week_start(None).map_err(anyhow::Error::msg)?.weekday(),
        chrono::Weekday::Mon
    );
    Ok(())
}

#[test]
fn week_start_names_the_bad_input() {
    let Err(message) = week_start(Some("12/11/2026")) else {
        panic!("a non-ISO date was accepted");
    };
    assert!(message.contains("12/11/2026"));
}

#[test]
fn figures_answer_reports_the_unused_share_and_bytes_sent_for_nothing() {
    let rows = vec![
        SupplyFigure {
            item_kind: "lesson".into(),
            route: "proxy".into(),
            supplied: 10,
            bytes: 1000,
            judged: 8,
            used: 2,
            corrected_anyway: 1,
            ignored: 5,
            bytes_ignored: 500,
        },
        SupplyFigure {
            item_kind: "card".into(),
            route: "tether".into(),
            supplied: 2,
            bytes: 3000,
            judged: 2,
            used: 1,
            corrected_anyway: 0,
            ignored: 1,
            bytes_ignored: 1500,
        },
    ];
    let answer = figures_answer(Utc::now(), 7, &rows);
    assert_eq!(answer["totals"]["supplied"], 12);
    assert_eq!(answer["totals"]["bytes_ignored"], 2000);
    assert_eq!(answer["totals"]["unused_share"], 0.6);
}

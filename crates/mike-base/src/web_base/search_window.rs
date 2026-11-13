use chrono::{DateTime, Duration, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid date '{value}': {source}")]
pub struct DateParseError {
    pub value: String,
    #[source]
    pub source: chrono::format::ParseError,
}

#[derive(Debug, Clone)]
pub struct SearchWindow {
    pub effective_from: Option<DateTime<Utc>>,
    pub effective_to: Option<DateTime<Utc>>,
    pub windowed: bool,
    pub defaulted: bool,
    pub widen_hint: String,
}

pub fn resolve_window(
    date_from: Option<&str>,
    date_to: Option<&str>,
    has_id_scope: bool,
) -> Result<SearchWindow, DateParseError> {
    let from = parse_optional(date_from)?;
    let to = parse_optional(date_to)?;

    if has_id_scope {
        let windowed = from.is_some() || to.is_some();
        let widen_hint = if windowed {
            "id-scoped search with explicit date filter applied".to_owned()
        } else {
            "id-scoped search — all time".to_owned()
        };
        return Ok(SearchWindow {
            effective_from: from,
            effective_to: to,
            windowed,
            defaulted: false,
            widen_hint,
        });
    }

    match (from, to) {
        (None, None) => {
            let now = Utc::now();
            let seven_days_ago = now - Duration::days(7);
            Ok(SearchWindow {
                effective_from: Some(seven_days_ago),
                effective_to: Some(now),
                windowed: true,
                defaulted: true,
                widen_hint: "showing last 7 days — pass date_from to widen".to_owned(),
            })
        }
        (from, to) => {
            let widen_hint = match (&from, &to) {
                (Some(f), Some(t)) => {
                    format!(
                        "searching {} to {}",
                        f.format("%Y-%m-%d"),
                        t.format("%Y-%m-%d")
                    )
                }
                (Some(f), None) => {
                    format!("searching from {} (no upper bound)", f.format("%Y-%m-%d"))
                }
                (None, Some(t)) => {
                    format!("searching up to {} (no lower bound)", t.format("%Y-%m-%d"))
                }
                (None, None) => unreachable!("matched above"),
            };
            Ok(SearchWindow {
                effective_from: from,
                effective_to: to,
                windowed: true,
                defaulted: false,
                widen_hint,
            })
        }
    }
}

fn parse_optional(s: Option<&str>) -> Result<Option<DateTime<Utc>>, DateParseError> {
    match s {
        None => Ok(None),
        Some(v) => {
            let dt = DateTime::parse_from_rfc3339(v)
                .map_err(|e| DateParseError {
                    value: v.to_owned(),
                    source: e,
                })?
                .with_timezone(&Utc);
            Ok(Some(dt))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_7d_no_dates_no_id_scope() -> anyhow::Result<()> {
        let before = Utc::now();
        let window = resolve_window(None, None, false).map_err(|e| anyhow::anyhow!("{e}"))?;
        let after = Utc::now();

        assert!(window.windowed, "must be windowed");
        assert!(window.defaulted, "must be defaulted");

        let from = window
            .effective_from
            .ok_or_else(|| anyhow::anyhow!("effective_from must be set"))?;
        let to = window
            .effective_to
            .ok_or_else(|| anyhow::anyhow!("effective_to must be set"))?;

        assert!(to >= before, "effective_to must be >= before");
        assert!(
            to <= after + Duration::seconds(1),
            "effective_to must be <= after"
        );

        let expected_from_lower = before - Duration::days(7) - Duration::seconds(1);
        let expected_from_upper = after - Duration::days(7) + Duration::seconds(1);
        assert!(from >= expected_from_lower, "effective_from too early");
        assert!(from <= expected_from_upper, "effective_from too late");

        assert!(
            window.widen_hint.contains("7 days"),
            "widen_hint should mention '7 days', got: {}",
            window.widen_hint
        );
        Ok(())
    }

    #[test]
    fn explicit_date_from_widens_beyond_7d() -> anyhow::Result<()> {
        let window = resolve_window(Some("2024-01-01T00:00:00Z"), None, false)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        assert!(window.windowed, "must be windowed");
        assert!(!window.defaulted, "must NOT be defaulted");

        let from = window
            .effective_from
            .ok_or_else(|| anyhow::anyhow!("effective_from must be set"))?;
        assert_eq!(from.format("%Y-%m-%d").to_string(), "2024-01-01");
        assert!(
            window.effective_to.is_none(),
            "no upper bound given — must be None"
        );
        Ok(())
    }

    #[test]
    fn explicit_both_dates_sets_window() -> anyhow::Result<()> {
        let window = resolve_window(
            Some("2025-06-01T00:00:00Z"),
            Some("2025-06-30T23:59:59Z"),
            false,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        assert!(window.windowed);
        assert!(!window.defaulted);

        let from = window
            .effective_from
            .ok_or_else(|| anyhow::anyhow!("from must be set"))?;
        let to = window
            .effective_to
            .ok_or_else(|| anyhow::anyhow!("to must be set"))?;
        assert_eq!(from.format("%Y-%m-%d").to_string(), "2025-06-01");
        assert_eq!(to.format("%Y-%m-%d").to_string(), "2025-06-30");
        assert!(
            window.widen_hint.contains("2025-06-01"),
            "hint should include from date"
        );
        Ok(())
    }

    #[test]
    fn explicit_date_to_only_sets_upper_bound() -> anyhow::Result<()> {
        let window = resolve_window(None, Some("2025-12-31T23:59:59Z"), false)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        assert!(window.windowed);
        assert!(!window.defaulted);
        assert!(window.effective_from.is_none(), "no lower bound given");
        let to = window
            .effective_to
            .ok_or_else(|| anyhow::anyhow!("to must be set"))?;
        assert_eq!(to.format("%Y-%m-%d").to_string(), "2025-12-31");
        Ok(())
    }

    #[test]
    fn id_scope_true_no_dates_no_default_window() -> anyhow::Result<()> {
        let window = resolve_window(None, None, true).map_err(|e| anyhow::anyhow!("{e}"))?;

        assert!(
            !window.windowed,
            "id-scoped with no dates must not be windowed"
        );
        assert!(!window.defaulted, "id-scoped must not be defaulted");
        assert!(window.effective_from.is_none());
        assert!(window.effective_to.is_none());
        Ok(())
    }

    #[test]
    fn id_scope_true_with_explicit_dates_is_windowed() -> anyhow::Result<()> {
        let window = resolve_window(
            Some("2025-01-01T00:00:00Z"),
            Some("2025-06-30T23:59:59Z"),
            true,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        assert!(
            window.windowed,
            "explicit dates set windowed=true even with id scope"
        );
        assert!(!window.defaulted);
        assert!(window.effective_from.is_some());
        assert!(window.effective_to.is_some());
        Ok(())
    }

    #[test]
    fn malformed_date_from_returns_error() -> anyhow::Result<()> {
        match resolve_window(Some("not-a-date"), None, false) {
            Err(err) => {
                assert!(
                    err.value == "not-a-date",
                    "error must carry the offending value"
                );
                let msg = err.to_string();
                assert!(
                    msg.contains("not-a-date"),
                    "error message must include the bad value"
                );
                Ok(())
            }
            Ok(_) => anyhow::bail!("expected error on bad date, got Ok"),
        }
    }

    #[test]
    fn malformed_date_to_returns_error() -> anyhow::Result<()> {
        match resolve_window(None, Some("2024-13-01"), false) {
            Err(err) => {
                assert_eq!(err.value, "2024-13-01");
                Ok(())
            }
            Ok(_) => anyhow::bail!("expected error on bad date, got Ok"),
        }
    }

    #[test]
    fn malformed_date_with_id_scope_returns_error() -> anyhow::Result<()> {
        match resolve_window(Some("bad"), None, true) {
            Err(err) => {
                assert_eq!(err.value, "bad");
                Ok(())
            }
            Ok(_) => anyhow::bail!("expected error on bad date, got Ok"),
        }
    }

    #[test]
    fn widen_hint_for_default_window_guides_widening() -> anyhow::Result<()> {
        let window = resolve_window(None, None, false).map_err(|e| anyhow::anyhow!("{e}"))?;
        assert!(
            window.widen_hint.contains("date_from"),
            "hint must name the param to widen, got: {}",
            window.widen_hint
        );
        Ok(())
    }
}

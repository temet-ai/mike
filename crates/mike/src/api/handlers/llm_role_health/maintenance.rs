use super::{CapabilityState, Finding, stamp};
use crate::store::persistence::lessons::maintenance_budget::MaintenanceRefusal;

const BUDGET_SETTING: &str = "judge_lessons.maintenance_max_per_day";

fn pass_label(pass: &str) -> &str {
    match pass {
        "shape" => "Lesson rewrite",
        "placement" => "Lesson placement",
        other => other,
    }
}

fn finding(refusal: &MaintenanceRefusal) -> Finding {
    let label = pass_label(&refusal.pass);
    let at = stamp(refusal.refused_at);
    if refusal.reason == "provider_limit" {
        return Finding {
            state: CapabilityState::Degraded,
            detail: format!("{label}: the LLM call limiter refused its model call at {at}."),
            fix: "Open /settings and raise the call ceiling for the lesson proposer role, or wait for the limiter window to reopen.".to_owned(),
        };
    }
    Finding {
        state: CapabilityState::Degraded,
        detail: format!(
            "{label}: its model call was refused at {at} because today's budget of {} maintenance calls is used up.",
            refusal.cap
        ),
        fix: format!(
            "Raise \"Lesson maintenance calls per day\" ({BUDGET_SETTING}) at /settings; the count starts again at 00:00 UTC."
        ),
    }
}

pub(super) fn findings(refusals: &[MaintenanceRefusal]) -> Vec<Finding> {
    refusals.iter().map(finding).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn refusal(pass: &str, reason: &str) -> MaintenanceRefusal {
        MaintenanceRefusal {
            pass: pass.to_owned(),
            reason: reason.to_owned(),
            cap: 400,
            refused_at: Utc
                .with_ymd_and_hms(2026, 10, 14, 12, 5, 0)
                .single()
                .unwrap_or_else(Utc::now),
        }
    }

    #[test]
    fn a_refusal_turns_an_otherwise_healthy_row_degraded_and_names_the_fix() {
        let now = Utc
            .with_ymd_and_hms(2026, 10, 14, 12, 30, 0)
            .single()
            .unwrap_or_else(Utc::now);
        let healthy = super::super::llm_role_health_row(
            now,
            &[],
            &[],
            &crate::feature_toggles::FeatureToggles::default(),
            &super::super::ProviderStatus::per_role(),
        );
        assert_eq!(healthy.state, CapabilityState::Active);
        let refused = super::super::llm_role_health_row(
            now,
            &[],
            &[refusal("shape", "daily_budget")],
            &crate::feature_toggles::FeatureToggles::default(),
            &super::super::ProviderStatus::per_role(),
        );
        assert_eq!(refused.state, CapabilityState::Degraded);
        assert!(
            refused.detail.contains("Lesson rewrite"),
            "{}",
            refused.detail
        );
        assert!(refused.fix.contains(BUDGET_SETTING), "{}", refused.fix);
    }

    #[test]
    fn findings_with_no_refusal_are_empty() {
        assert!(findings(&[]).is_empty());
    }

    #[test]
    fn daily_budget_refusal_names_the_pass_the_cap_and_the_setting() {
        let found = findings(&[refusal("placement", "daily_budget")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].state, CapabilityState::Degraded);
        assert!(found[0].detail.contains("Lesson placement"));
        assert!(found[0].detail.contains("2026-10-14 12:05 UTC"));
        assert!(found[0].detail.contains("400"));
        assert!(found[0].fix.contains(BUDGET_SETTING));
    }

    #[test]
    fn provider_limit_refusal_points_at_the_limiter_not_the_budget() {
        let found = findings(&[refusal("shape", "provider_limit")]);
        assert!(found[0].detail.contains("Lesson rewrite"));
        assert!(found[0].detail.contains("limiter"));
        assert!(!found[0].fix.contains(BUDGET_SETTING));
    }
}

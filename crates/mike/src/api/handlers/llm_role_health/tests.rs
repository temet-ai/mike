use super::*;
use crate::llm::provider::LlmProvider;
use chrono::TimeZone;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

fn outcome(role: &str, hours_ago: i64, last: &str, streak: i64) -> LlmRoleOutcome {
    let at = now() - Duration::hours(hours_ago);
    LlmRoleOutcome {
        role: role.to_owned(),
        model: "google/gemini-3.8-flash".to_owned(),
        last_outcome: last.to_owned(),
        last_http_status: (last == "http_error").then_some(402),
        last_call_at: at,
        last_ok_at: (streak == 0).then_some(at),
        failed_streak: streak,
    }
}

fn on(f: impl FnOnce(&mut FeatureToggles)) -> FeatureToggles {
    let mut toggles = FeatureToggles::default();
    f(&mut toggles);
    toggles
}

#[test]
fn row_three_refusals_in_a_row_is_inert_with_the_top_up_fix() {
    let outcomes = [outcome("judge_worker", 20, "http_error", 5)];
    let r = llm_role_health_row(
        now(),
        &outcomes,
        &[],
        &FeatureToggles::default(),
        &ProviderStatus::per_role(),
    );
    assert_eq!(r.state, CapabilityState::Inert);
    assert!(r.detail.contains("Turn judge: the last 5 call(s) failed"));
    assert!(r.detail.contains("HTTP 402 Payment Required"));
    assert!(r.detail.contains("no call succeeded in the last 7 days"));
    assert!(r.fix.contains(TOP_UP_URL), "{}", r.fix);
}

#[test]
fn row_a_single_failure_after_successes_is_degraded() {
    let outcomes = [outcome("steerer", 1, "timeout", 1)];
    let r = llm_role_health_row(
        now(),
        &outcomes,
        &[],
        &FeatureToggles::default(),
        &ProviderStatus::per_role(),
    );
    assert_eq!(r.state, CapabilityState::Degraded);
    assert!(r.detail.contains("(timeout)"), "{}", r.detail);
}

#[test]
fn row_codex_transport_failure_points_at_codex_login() {
    let mut o = outcome("session_judge", 2, "transport_error", 4);
    o.model = "codex/gpt-6-sol".to_owned();
    let r = llm_role_health_row(
        now(),
        &[o],
        &[],
        &FeatureToggles::default(),
        &ProviderStatus::per_role(),
    );
    assert_eq!(r.state, CapabilityState::Inert);
    assert!(r.fix.contains("codex login status"), "{}", r.fix);
}

#[test]
fn row_switched_on_role_with_no_call_in_the_window_is_degraded() {
    let toggles = on(|t| t.judge_lessons = true);
    let r = llm_role_health_row(now(), &[], &[], &toggles, &ProviderStatus::per_role());
    assert_eq!(r.state, CapabilityState::Degraded);
    assert!(
        r.detail
            .contains("Lesson proposer is switched on but has made no LLM call in the last 7 days"),
        "{}",
        r.detail
    );
}

#[test]
fn row_switched_on_role_quiet_past_its_interval_is_degraded() {
    let toggles = on(|t| t.judge_worker = true);
    let outcomes = [outcome("judge_worker", 60, "ok", 0)];
    let r = llm_role_health_row(now(), &outcomes, &[], &toggles, &ProviderStatus::per_role());
    assert_eq!(r.state, CapabilityState::Degraded);
    assert!(r.detail.contains("for 2 days"), "{}", r.detail);
}

#[test]
fn row_switched_on_role_inside_its_interval_is_active() {
    let toggles = on(|t| t.judge_worker = true);
    let outcomes = [outcome("judge_worker", 30, "ok", 0)];
    let r = llm_role_health_row(now(), &outcomes, &[], &toggles, &ProviderStatus::per_role());
    assert_eq!(r.state, CapabilityState::Active);
    assert!(r.detail.contains("Turn judge"), "{}", r.detail);
}

#[test]
fn row_switched_off_role_with_no_calls_is_not_judged() {
    let r = llm_role_health_row(
        now(),
        &[],
        &[],
        &FeatureToggles::default(),
        &ProviderStatus::per_role(),
    );
    assert_eq!(r.state, CapabilityState::Active);
    assert!(r.fix.is_empty());
}

#[test]
fn row_shared_fix_is_listed_once() {
    let outcomes = [
        outcome("judge_worker", 3, "http_error", 4),
        outcome("session_judge", 3, "http_error", 4),
    ];
    let r = llm_role_health_row(
        now(),
        &outcomes,
        &[],
        &FeatureToggles::default(),
        &ProviderStatus::per_role(),
    );
    assert_eq!(r.fix.matches(TOP_UP_URL).count(), 1, "{}", r.fix);
}

fn provider(provider: LlmProvider, problems: Vec<ProviderProblem>) -> ProviderStatus {
    ProviderStatus { provider, problems }
}

#[test]
fn row_names_the_active_provider_when_nothing_is_wrong() {
    let r = llm_role_health_row(
        now(),
        &[],
        &[],
        &FeatureToggles::default(),
        &provider(LlmProvider::Claude, Vec::new()),
    );
    assert_eq!(r.state, CapabilityState::Active);
    assert!(r.detail.starts_with("LLM provider: Claude"), "{}", r.detail);
}

#[test]
fn row_per_role_mode_says_so() {
    let r = llm_role_health_row(
        now(),
        &[],
        &[],
        &FeatureToggles::default(),
        &ProviderStatus::per_role(),
    );
    assert!(
        r.detail.starts_with("LLM provider: per role"),
        "{}",
        r.detail
    );
}

#[test]
fn row_provider_without_a_login_is_inert_and_carries_its_fix() {
    let missing = ProviderProblem {
        detail: "llm.provider is claude but the machine's Claude Code login is missing".to_owned(),
        fix: "Run `claude auth login`, then check `claude auth status`.".to_owned(),
    };
    let r = llm_role_health_row(
        now(),
        &[],
        &[],
        &FeatureToggles::default(),
        &provider(LlmProvider::Claude, vec![missing]),
    );
    assert_eq!(r.state, CapabilityState::Inert);
    assert!(
        r.detail.contains("Claude Code login is missing"),
        "{}",
        r.detail
    );
    assert!(r.fix.contains("claude auth status"), "{}", r.fix);
}

#[test]
fn row_claude_transport_failure_points_at_claude_auth_status() {
    let mut o = outcome("steerer", 2, "transport_error", 4);
    o.model = "claude/sonnet".to_owned();
    let r = llm_role_health_row(
        now(),
        &[o],
        &[],
        &FeatureToggles::default(),
        &ProviderStatus::per_role(),
    );
    assert_eq!(r.state, CapabilityState::Inert);
    assert!(r.fix.contains("claude auth status"), "{}", r.fix);
}

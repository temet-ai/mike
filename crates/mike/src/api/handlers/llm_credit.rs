use super::*;
use crate::api::usage::OpenRouterUsageResponse;
use crate::store::persistence::llm_calls::LlmCallStatus;

const PAYMENT_REQUIRED: i32 = 402;
const RECENT_CALLS: i64 = 20;
const TOP_UP_URL: &str = "https://openrouter.ai/settings/credits";
const LOW_BALANCE_DAYS_THRESHOLD: f64 = 3.0;

fn row(state: CapabilityState, detail: String, fix: String) -> CapabilityRow {
    CapabilityRow {
        id: "llm_credit",
        label: "LLM worker credit",
        state,
        detail,
        fix,
    }
}

fn refused_streak(recent: &[LlmCallStatus]) -> &[LlmCallStatus] {
    let streak = recent
        .iter()
        .take_while(|call| call.http_status == Some(PAYMENT_REQUIRED))
        .count();
    &recent[..streak]
}

pub(crate) fn llm_credit_row(recent: &[LlmCallStatus]) -> CapabilityRow {
    let streak = refused_streak(recent);
    let Some(oldest) = streak.last() else {
        let detail = if recent.is_empty() {
            "No LLM worker call in the last 24 hours, so there is no recent answer from OpenRouter to judge credit by".to_owned()
        } else {
            "OpenRouter accepted the most recent LLM worker call".to_owned()
        };
        return row(CapabilityState::Active, detail, String::new());
    };
    let since = oldest.created_at.format("%Y-%m-%d %H:%M UTC");
    row(
        CapabilityState::Inert,
        format!(
            "OpenRouter has refused the last {} LLM call(s) since {since} with 402 Payment Required: the account is out of credit. The turn judge, session judge, steerers, turn guard and eval runs cannot run until it is topped up.",
            streak.len()
        ),
        format!(
            "Add credit at {TOP_UP_URL}. Workers resume on their next call; nothing needs restarting."
        ),
    )
}

pub(crate) async fn llm_credit_capability(state: &AppState) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return row(
            CapabilityState::Off,
            "Persistence is disabled, so LLM call outcomes are not recorded".to_owned(),
            String::new(),
        );
    };
    let base = match persist.recent_llm_call_statuses(RECENT_CALLS).await {
        Ok(recent) => llm_credit_row(&recent),
        Err(e) => {
            return row(
                CapabilityState::Degraded,
                format!("Could not read recent LLM call outcomes: {e}"),
                "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
            );
        }
    };
    if base.state == CapabilityState::Inert {
        return base;
    }
    apply_balance_check(state, base).await
}

async fn apply_balance_check(state: &AppState, base: CapabilityRow) -> CapabilityRow {
    match state.openrouter_usage_cache().peek().await {
        Some(snapshot) => low_balance_row(&snapshot).unwrap_or(base),
        None => {
            spawn_usage_refresh(state);
            base
        }
    }
}

fn spawn_usage_refresh(state: &AppState) {
    use secrecy::ExposeSecret;
    let state = state.clone();
    state.tracker().spawn(async move {
        let api_key = state.openrouter_key().map(|k| k.expose_secret().to_owned());
        let _ = state
            .openrouter_usage_cache()
            .get(
                &state.client,
                &state.config.steerer.openrouter_base_url,
                api_key.as_deref(),
                state.persist(),
            )
            .await;
    });
}

fn low_balance_row(snapshot: &OpenRouterUsageResponse) -> Option<CapabilityRow> {
    if !snapshot.configured {
        return None;
    }
    let balance_usd = snapshot.credits_remaining?;
    let avg_daily_spend_7d_usd = snapshot.avg_daily_spend_7d_usd.unwrap_or(0.0);
    let low_runway = snapshot
        .days_left
        .is_some_and(|d| d < LOW_BALANCE_DAYS_THRESHOLD);
    let empty_balance = balance_usd <= 0.0;
    if !(low_runway || empty_balance) {
        return None;
    }
    let runway = match snapshot.days_left {
        Some(d) => format!("about {d:.0} days"),
        None => "an unknown number of days".to_owned(),
    };
    Some(row(
        CapabilityState::Degraded,
        format!(
            "OpenRouter balance is ${balance_usd:.2} — {runway} at the last 7 days' average of ${avg_daily_spend_7d_usd:.2}/day."
        ),
        format!(
            "Add credit at {TOP_UP_URL}, or see the spend breakdown at /settings/integrations."
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(status: Option<i32>, minute: u32) -> LlmCallStatus {
        LlmCallStatus {
            http_status: status,
            created_at: Utc
                .with_ymd_and_hms(2026, 9, 28, 9, minute, 0)
                .single()
                .unwrap_or_else(Utc::now),
        }
    }

    #[test]
    fn llm_credit_latest_call_refused_is_inert_with_top_up_fix() {
        let recent = [
            call(Some(402), 47),
            call(Some(402), 46),
            call(Some(200), 44),
        ];
        let r = llm_credit_row(&recent);
        assert_eq!(r.state, CapabilityState::Inert);
        assert!(
            r.detail
                .contains("last 2 LLM call(s) since 2026-09-28 09:46 UTC")
        );
        assert!(r.fix.contains(TOP_UP_URL));
    }

    #[test]
    fn llm_credit_latest_call_accepted_is_active_despite_older_refusals() {
        let recent = [call(Some(200), 50), call(Some(402), 47)];
        let r = llm_credit_row(&recent);
        assert_eq!(r.state, CapabilityState::Active);
        assert!(r.fix.is_empty());
    }

    #[test]
    fn llm_credit_other_http_errors_are_not_credit_problems() {
        let recent = [call(Some(500), 50), call(None, 49)];
        assert_eq!(llm_credit_row(&recent).state, CapabilityState::Active);
    }

    #[test]
    fn llm_credit_no_recent_calls_says_so() {
        let r = llm_credit_row(&[]);
        assert_eq!(r.state, CapabilityState::Active);
        assert!(r.detail.contains("No LLM worker call in the last 24 hours"));
    }

    fn snapshot(
        configured: bool,
        balance_usd: Option<f64>,
        avg_daily_spend_7d_usd: f64,
    ) -> OpenRouterUsageResponse {
        let days_left = balance_usd
            .filter(|_| avg_daily_spend_7d_usd > 0.0)
            .map(|b| b / avg_daily_spend_7d_usd);
        OpenRouterUsageResponse {
            configured,
            label: None,
            limit: None,
            limit_remaining: None,
            limit_reset: None,
            usage_total: None,
            usage_daily: None,
            usage_weekly: None,
            usage_monthly: None,
            credits_total: None,
            credits_used: None,
            credits_remaining: balance_usd,
            is_free_tier: None,
            credits_unavailable: false,
            recorded_month_usd: None,
            unrecorded_month_usd: None,
            avg_daily_spend_7d_usd: Some(avg_daily_spend_7d_usd),
            days_left,
            top_up_url: Some(TOP_UP_URL),
        }
    }

    #[test]
    fn low_balance_row_none_when_snapshot_unconfigured() {
        let s = snapshot(false, Some(0.0), 1.0);
        assert!(low_balance_row(&s).is_none());
    }

    #[test]
    fn low_balance_row_none_when_credits_unavailable() {
        let s = snapshot(true, None, 1.0);
        assert!(low_balance_row(&s).is_none());
    }

    #[test]
    fn low_balance_row_none_when_runway_is_healthy() {
        let s = snapshot(true, Some(10.0), 1.0);
        assert!(low_balance_row(&s).is_none());
    }

    #[test]
    fn low_balance_row_degraded_when_days_left_below_threshold() -> anyhow::Result<()> {
        let s = snapshot(true, Some(2.0), 1.0);
        let r = low_balance_row(&s).ok_or_else(|| anyhow::anyhow!("low runway must degrade"))?;
        assert_eq!(r.state, CapabilityState::Degraded);
        assert!(r.detail.contains("about 2 days"));
        assert!(r.fix.contains(TOP_UP_URL));
        assert!(r.fix.contains("/settings/integrations"));
        Ok(())
    }

    #[test]
    fn low_balance_row_degraded_when_balance_nonpositive_even_without_avg_spend()
    -> anyhow::Result<()> {
        let s = snapshot(true, Some(-0.19), 0.0);
        let r = low_balance_row(&s).ok_or_else(|| anyhow::anyhow!("empty balance must degrade"))?;
        assert_eq!(r.state, CapabilityState::Degraded);
        assert!(r.detail.contains("an unknown number of days"));
        Ok(())
    }
}

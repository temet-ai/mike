use super::*;

#[cfg(test)]
mod capabilities_tests {
    use super::*;
    use secrecy::SecretString;

    async fn state_with_configured_key() -> anyhow::Result<AppState> {
        crate::llm::health::reset_for_test();
        let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
        state.feature_toggle_overlay.store(std::sync::Arc::new(
            crate::feature_toggles::FeatureToggles {
                steerer: true,
                ..crate::feature_toggles::FeatureToggles::default()
            },
        ));
        state
            .steerer_openrouter_key
            .store(std::sync::Arc::new(Some(SecretString::new(
                "sk-or-test-key".to_owned().into(),
            ))));
        Ok(state)
    }

    fn steering_row(payload: &CapabilitiesResponse) -> anyhow::Result<&CapabilityRow> {
        payload
            .capabilities
            .iter()
            .find(|row| row.id == "steering")
            .ok_or_else(|| anyhow::anyhow!("capabilities response must carry a steering row"))
    }

    #[tokio::test]
    async fn steering_active_when_key_configured_and_no_failures_recorded() -> anyhow::Result<()> {
        let state = state_with_configured_key().await?;
        let Json(payload) = capabilities(State(state)).await;
        let row = steering_row(&payload)?;
        assert_eq!(row.state, CapabilityState::Active);
        assert!(row.fix.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn steering_inert_on_first_permanent_failure() -> anyhow::Result<()> {
        let state = state_with_configured_key().await?;
        crate::llm::health::record_failure(crate::llm::health::test_detail(
            Some(402),
            "LLM HTTP error: status 402",
        ));

        let Json(payload) = capabilities(State(state)).await;
        let row = steering_row(&payload)?;
        assert_eq!(row.state, CapabilityState::Inert);
        assert!(
            row.detail.contains("402"),
            "detail must name the HTTP status: {}",
            row.detail
        );
        assert!(
            row.fix.contains("credit") || row.fix.contains("key"),
            "fix must offer a concrete remedy: {}",
            row.fix
        );
        Ok(())
    }

    #[tokio::test]
    async fn steering_inert_on_first_invalid_key_failure() -> anyhow::Result<()> {
        let state = state_with_configured_key().await?;
        crate::llm::health::record_failure(crate::llm::health::test_detail(
            Some(401),
            "LLM HTTP error: status 401",
        ));

        let Json(payload) = capabilities(State(state)).await;
        let row = steering_row(&payload)?;
        assert_eq!(row.state, CapabilityState::Inert);
        assert!(
            row.detail.contains("401"),
            "detail must name the HTTP status: {}",
            row.detail
        );
        assert!(
            row.fix.contains("rotate") || row.fix.contains("key"),
            "fix must offer a concrete remedy: {}",
            row.fix
        );
        Ok(())
    }

    #[tokio::test]
    async fn steering_degraded_on_repeated_transient_failure_never_escalates_to_inert()
    -> anyhow::Result<()> {
        let state = state_with_configured_key().await?;
        for _ in 0..5 {
            crate::llm::health::record_failure(crate::llm::health::test_detail(
                Some(429),
                "LLM HTTP error: status 429",
            ));
        }

        let Json(payload) = capabilities(State(state)).await;
        let row = steering_row(&payload)?;
        assert_eq!(row.state, CapabilityState::Degraded);
        assert!(
            row.detail.contains("429"),
            "detail must name the HTTP status: {}",
            row.detail
        );
        assert!(
            !row.fix.is_empty(),
            "a degraded row must still carry a remedy"
        );
        Ok(())
    }

    #[tokio::test]
    async fn steering_returns_to_active_after_recorded_success() -> anyhow::Result<()> {
        let state = state_with_configured_key().await?;
        crate::llm::health::record_failure(crate::llm::health::test_detail(
            Some(402),
            "LLM HTTP error: status 402",
        ));
        crate::llm::health::record_success();

        let Json(payload) = capabilities(State(state)).await;
        let row = steering_row(&payload)?;
        assert_eq!(row.state, CapabilityState::Active);
        assert!(row.fix.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn steering_inert_when_no_key_configured() -> anyhow::Result<()> {
        crate::llm::health::reset_for_test();
        let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
        state.feature_toggle_overlay.store(std::sync::Arc::new(
            crate::feature_toggles::FeatureToggles {
                steerer: true,
                ..crate::feature_toggles::FeatureToggles::default()
            },
        ));
        state
            .steerer_openrouter_key
            .store(std::sync::Arc::new(None));

        let Json(payload) = capabilities(State(state)).await;
        let row = steering_row(&payload)?;
        assert_eq!(row.state, CapabilityState::Inert);
        assert!(row.detail.contains("no OpenRouter API key"));
        Ok(())
    }

    #[tokio::test]
    async fn health_llm_field_reflects_recorded_failures() -> anyhow::Result<()> {
        let state = state_with_configured_key().await?;
        crate::llm::health::record_failure(crate::llm::health::test_detail(
            Some(402),
            "LLM HTTP error: status 402",
        ));

        let Json(body) = health(State(state)).await;
        assert!(body.llm.key_configured);
        assert_eq!(body.llm.consecutive_failures, 1);
        assert_eq!(body.llm.last_failure_status, Some(402));
        assert!(
            body.llm.last_failure_at.is_some(),
            "a recorded failure must carry a timestamp"
        );
        Ok(())
    }

    #[tokio::test]
    async fn health_persistence_field_reports_disabled_when_not_configured() -> anyhow::Result<()> {
        let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
        let Json(body) = health(State(state)).await;
        assert!(!body.persistence.enabled);
        assert!(!body.persistence.reachable);
        assert!(
            body.persistence.latest_capture_write_at.is_none(),
            "no PersistHandle means no capture-staleness signal to report"
        );
        assert!(body.persistence.capture_write_age_secs.is_none());
        assert!(
            !body.persistence.capture_stalled,
            "no capture pipeline exists to stall when persistence is disabled"
        );
        Ok(())
    }

    #[test]
    fn capture_stalled_true_when_traffic_recent_and_writes_stale() {
        let now = chrono::Utc::now();
        let last_request_at = Some(now - chrono::Duration::seconds(10));
        let capture_write_age_secs = Some(CAPTURE_STALL_WRITE_AGE_SECS + 180);

        let stalled =
            compute_capture_stalled(true, last_request_at, capture_write_age_secs, 600, now);
        assert!(
            stalled,
            "recent traffic with a stale capture write must read as a real outage"
        );
        assert_eq!(
            compute_health_status(
                SubsystemReadiness::new(true, true),
                SubsystemReadiness::new(false, false),
                0,
                false,
                stalled
            ),
            "degraded",
            "a stalled capture path must not report ok even though the pool itself is reachable"
        );
    }

    #[test]
    fn llm_failures_do_not_degrade_status_when_setting_is_off() {
        assert_eq!(
            compute_health_status(
                SubsystemReadiness::new(true, true),
                SubsystemReadiness::new(false, false),
                7,
                false,
                false
            ),
            "ok",
            "LLM failures must not fold into the aggregate while the opt-in setting is off"
        );
    }

    #[test]
    fn llm_failures_degrade_status_when_setting_is_on() {
        assert_eq!(
            compute_health_status(
                SubsystemReadiness::new(true, true),
                SubsystemReadiness::new(false, false),
                1,
                true,
                false
            ),
            "degraded",
            "opting in must restore the pre-change coupling exactly"
        );
        assert_eq!(
            compute_health_status(
                SubsystemReadiness::new(true, true),
                SubsystemReadiness::new(false, false),
                0,
                true,
                false
            ),
            "ok",
            "opting in must not degrade on a healthy LLM tracker"
        );
    }

    #[test]
    fn capture_stalled_false_when_no_recent_traffic_idle_not_stalled() {
        let now = chrono::Utc::now();
        let last_request_at = Some(now - chrono::Duration::seconds(600));
        let capture_write_age_secs = Some(9_000);

        let stalled =
            compute_capture_stalled(true, last_request_at, capture_write_age_secs, 20_000, now);
        assert!(
            !stalled,
            "no traffic in the last {CAPTURE_STALL_TRAFFIC_WINDOW_SECS}s means idle, not stalled"
        );
        assert_eq!(
            compute_health_status(
                SubsystemReadiness::new(true, true),
                SubsystemReadiness::new(false, false),
                0,
                false,
                stalled
            ),
            "ok"
        );
    }

    #[test]
    fn capture_stalled_false_when_traffic_recent_and_writes_fresh_healthy() {
        let now = chrono::Utc::now();
        let last_request_at = Some(now - chrono::Duration::seconds(5));
        let capture_write_age_secs = Some(3);

        let stalled =
            compute_capture_stalled(true, last_request_at, capture_write_age_secs, 600, now);
        assert!(!stalled);
        assert_eq!(
            compute_health_status(
                SubsystemReadiness::new(true, true),
                SubsystemReadiness::new(false, false),
                0,
                false,
                stalled
            ),
            "ok"
        );
    }

    #[test]
    fn capture_stalled_respects_startup_grace_window() {
        let now = chrono::Utc::now();
        let last_request_at = Some(now);

        assert!(
            !compute_capture_stalled(
                true,
                last_request_at,
                None,
                CAPTURE_STALL_STARTUP_GRACE_SECS.saturating_sub(1),
                now
            ),
            "inside the startup grace window, no write yet is not a stall"
        );
        assert!(
            compute_capture_stalled(
                true,
                last_request_at,
                None,
                CAPTURE_STALL_STARTUP_GRACE_SECS + 600,
                now
            ),
            "past the startup grace window, zero capture writes ever while \
             handling traffic is a stalled write path"
        );
    }

    #[test]
    fn capture_stalled_false_when_persistence_disabled() {
        let now = chrono::Utc::now();
        assert!(!compute_capture_stalled(false, Some(now), None, 600, now));
    }

    #[test]
    fn capture_stalled_false_when_only_uncapturable_traffic_arrived() {
        let now = chrono::Utc::now();
        let no_capturable_request_yet = None;
        assert!(
            !compute_capture_stalled(true, no_capturable_request_yet, Some(9_000), 20_000, now),
            "remote-control polls and header-less requests are traffic the capture path never \
             sees, so they prove nothing about it"
        );
    }

    #[test]
    fn capture_stall_fix_names_the_silence_only_when_stalled() {
        assert!(capture_stall_fix(false, Some(400)).is_none());
        let hint = capture_stall_fix(true, Some(400)).unwrap_or_default();
        assert!(hint.contains("400s old"), "{hint}");
        assert!(hint.contains("start-proxy.sh"), "{hint}");
        let never = capture_stall_fix(true, None).unwrap_or_default();
        assert!(never.contains("since the proxy started"), "{never}");
    }

    #[tokio::test]
    async fn capabilities_persistence_row_reports_off_when_not_configured() -> anyhow::Result<()> {
        let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
        let Json(payload) = capabilities(State(state)).await;
        let row = payload
            .capabilities
            .iter()
            .find(|row| row.id == "persistence")
            .ok_or_else(|| anyhow::anyhow!("capabilities response must carry a persistence row"))?;
        assert_eq!(row.state, CapabilityState::Off);
        Ok(())
    }

    #[tokio::test]
    async fn capability_fixes_name_the_page_that_holds_the_control() -> anyhow::Result<()> {
        let mut config = crate::config::ProxyConfig::for_tests_in_memory();
        config.embedding.enabled = false;
        let state = AppState::new(config).await?;
        state.feature_toggle_overlay.store(std::sync::Arc::new(
            crate::feature_toggles::FeatureToggles {
                steerer: false,
                weasel_intercept: false,
                ..Default::default()
            },
        ));
        let Json(payload) = capabilities(State(state)).await;

        for (id, owning_page) in [
            ("steering", "/settings/steering"),
            ("semantic_recall", "/settings/embedding"),
            ("weasel_intercept", "/settings/flagging"),
        ] {
            let row = payload
                .capabilities
                .iter()
                .find(|row| row.id == id)
                .ok_or_else(|| anyhow::anyhow!("capabilities response must carry a {id} row"))?;
            assert!(
                row.fix.contains(owning_page),
                "{id} fix must name {owning_page}, got: {}",
                row.fix
            );
        }

        for row in &payload.capabilities {
            let hub_only = row
                .fix
                .match_indices("/settings")
                .any(|(at, _)| !row.fix[at + "/settings".len()..].starts_with('/'));
            assert!(!hub_only, "{} fix names the bare hub: {}", row.id, row.fix);
        }
        Ok(())
    }
}

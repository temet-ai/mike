use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CapabilityState {
    Active,
    Degraded,
    Inert,
    Off,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilityRow {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) state: CapabilityState,
    pub(crate) detail: String,
    pub(crate) fix: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CapabilitiesResponse {
    pub(crate) capabilities: Vec<CapabilityRow>,
}

pub(crate) async fn capabilities(State(state): State<AppState>) -> Json<CapabilitiesResponse> {
    let toggles = state.feature_toggle_overlay.load();
    let steering = steering_capability(&state, toggles.steerer).await;
    let semantic_recall = semantic_recall_capability(&state).await;
    let knowledge = crate::api::project_knowledge::embedding_capability(&state).await;
    let weasel_intercept = weasel_intercept_capability(&state, toggles.weasel_intercept);
    let persistence = persistence_capability(&state).await;
    let llm_credit = llm_credit_capability(&state).await;
    let remote_control_link = remote_control_link_capability(&state).await;
    let llm_worker_lease = llm_worker_lease_capability(&state).await;
    let llm_role_health = llm_role_health_capability(&state, &toggles).await;
    let worker_toggles = worker_toggles_capability(&state).await;
    let scheduled_jobs = scheduled_jobs_capability(&state).await;
    let db_cost = crate::db_cost::health::capability_row(
        state.persist().is_some(),
        crate::db_cost::current().as_deref(),
    );

    Json(CapabilitiesResponse {
        capabilities: vec![
            steering,
            semantic_recall,
            weasel_intercept,
            persistence,
            knowledge,
            llm_credit,
            remote_control_link,
            llm_worker_lease,
            llm_role_health,
            worker_toggles,
            scheduled_jobs,
            db_cost,
        ],
    })
}

async fn steering_capability(state: &AppState, enabled: bool) -> CapabilityRow {
    if !enabled {
        return CapabilityRow {
            id: "steering",
            label: "Steering & judges",
            state: CapabilityState::Off,
            detail: "Steering is disabled via the steerer feature toggle".to_owned(),
            fix: "Enable the steerer feature toggle at /settings/steering".to_owned(),
        };
    }
    let models = steerer_job_models(state);
    let providers = unique_providers(&models);
    let mut problems = Vec::new();
    for provider in &providers {
        if let Some(problem) = provider_problem(state, *provider).await {
            problems.push((*provider, problem));
        }
    }
    if problems.is_empty() {
        return steering_runtime_capability(state);
    }
    let detail = problems
        .iter()
        .map(|(provider, problem)| {
            format!(
                "Steerers on {} cannot run: {}",
                provider.display_name(),
                problem.detail
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let all_down = problems.len() == providers.len();
    CapabilityRow {
        id: "steering",
        label: "Steering & judges",
        state: if all_down {
            CapabilityState::Inert
        } else {
            CapabilityState::Degraded
        },
        detail: format!(
            "Steering is enabled but {detail} — evaluations on an unavailable provider fail fast and skip"
        ),
        fix: problems
            .iter()
            .map(|(_, problem)| problem.fix.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn steering_runtime_capability(state: &AppState) -> CapabilityRow {
    let health = crate::llm::health::snapshot();
    if health.consecutive_failures == 0 {
        return CapabilityRow {
            id: "steering",
            label: "Steering & judges",
            state: CapabilityState::Active,
            detail: format!(
                "Steering active — evaluating enrolled sessions with {}",
                state
                    .settings_overlay
                    .load()
                    .live_steerer_model(&state.config.steerer.model)
            ),
            fix: String::new(),
        };
    }
    let failures = health.consecutive_failures;
    let (why, fix) = health
        .last_failure
        .as_ref()
        .map(describe_steering_failure)
        .unwrap_or_else(|| {
            (
                "no failure detail was recorded".to_owned(),
                "Check OpenRouter connectivity and the steerer model at /settings/steering"
                    .to_owned(),
            )
        });
    let permanent = health
        .last_failure
        .as_ref()
        .is_some_and(|detail| is_permanent_steering_failure(detail.status));
    CapabilityRow {
        id: "steering",
        label: "Steering & judges",
        state: if permanent {
            CapabilityState::Inert
        } else {
            CapabilityState::Degraded
        },
        detail: if permanent {
            format!("Steering is inert — {why} ({failures} consecutive evaluation failure(s))")
        } else {
            format!("Steering degraded — {failures} consecutive evaluation failures; {why}")
        },
        fix,
    }
}

async fn semantic_recall_capability(state: &AppState) -> CapabilityRow {
    let embedding_service = state.embedding_service();
    if !embedding_service.is_available() {
        return CapabilityRow {
            id: "semantic_recall",
            label: "Semantic recall",
            state: CapabilityState::Off,
            detail: "Semantic recall is disabled via embedding.enabled".to_owned(),
            fix: "Enable embedding.enabled at /settings/embedding".to_owned(),
        };
    }
    let url = embedding_service.url();
    if probe_embedding_reachable(&embedding_service).await {
        CapabilityRow {
            id: "semantic_recall",
            label: "Semantic recall",
            state: CapabilityState::Active,
            detail: format!("Semantic recall active against the embedding server at {url}"),
            fix: String::new(),
        }
    } else {
        CapabilityRow {
            id: "semantic_recall",
            label: "Semantic recall",
            state: CapabilityState::Degraded,
            detail: format!(
                "Embedding server at {url} is unreachable — recall is falling back to full-text search"
            ),
            fix: format!("Check that the embedding server is running at {url}"),
        }
    }
}

fn weasel_intercept_capability(state: &AppState, enabled: bool) -> CapabilityRow {
    let forbidden_word_count = state
        .settings_overlay
        .load()
        .weasel_intercept_forbidden_words
        .len();
    if !enabled {
        CapabilityRow {
            id: "weasel_intercept",
            label: "Weasel intercept",
            state: CapabilityState::Off,
            detail: "Weasel intercept is disabled via the weasel_intercept feature toggle"
                .to_owned(),
            fix: "Enable the weasel_intercept feature toggle at /settings/flagging".to_owned(),
        }
    } else if forbidden_word_count == 0 {
        CapabilityRow {
            id: "weasel_intercept",
            label: "Weasel intercept",
            state: CapabilityState::Inert,
            detail: "Weasel intercept is enabled but no forbidden words are configured — \
                     the response scan never matches"
                .to_owned(),
            fix: "Add forbidden words at /settings/flagging".to_owned(),
        }
    } else {
        CapabilityRow {
            id: "weasel_intercept",
            label: "Weasel intercept",
            state: CapabilityState::Active,
            detail: format!(
                "Weasel intercept active — scanning streamed responses against \
                 {forbidden_word_count} forbidden word(s); non-streamed responses are recorded \
                 as not scanned"
            ),
            fix: String::new(),
        }
    }
}

async fn persistence_capability(state: &AppState) -> CapabilityRow {
    if let Some(persist) = state.persist() {
        if probe_persistence_reachable(persist).await {
            CapabilityRow {
                id: "persistence",
                label: "Persistence",
                state: CapabilityState::Active,
                detail: "Connected to Postgres — sessions are captured to durable storage"
                    .to_owned(),
                fix: String::new(),
            }
        } else {
            CapabilityRow {
                id: "persistence",
                label: "Persistence",
                state: CapabilityState::Degraded,
                detail: "Persistence is enabled but the database is unreachable — captures are \
                         failing"
                    .to_owned(),
                fix: "Check that Postgres is reachable at the configured MIKE_DATABASE_URL"
                    .to_owned(),
            }
        }
    } else {
        CapabilityRow {
            id: "persistence",
            label: "Persistence",
            state: CapabilityState::Off,
            detail: "Persistence is disabled (MIKE_PERSIST_ENABLED=0) — running ephemeral, \
                     in-memory only"
                .to_owned(),
            fix: "Set MIKE_PERSIST_ENABLED=1 and configure MIKE_DATABASE_URL".to_owned(),
        }
    }
}

#[cfg(test)]
mod weasel_capability_tests {
    use std::sync::Arc;

    use super::*;
    use crate::feature_toggles::FeatureToggles;
    use crate::settings::SettingsOverlay;

    async fn state_with_live_overlay(words: &[&str]) -> anyhow::Result<AppState> {
        let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
        assert!(
            state.config.weasel_intercept.forbidden_words.is_empty(),
            "the boot config must hold no forbidden words for these tests to mean anything"
        );
        state.feature_toggle_overlay.store(Arc::new(FeatureToggles {
            weasel_intercept: true,
            ..FeatureToggles::default()
        }));
        state.settings_overlay.store(Arc::new(SettingsOverlay {
            weasel_intercept_forbidden_words: words.iter().map(|w| (*w).to_owned()).collect(),
            ..SettingsOverlay::default()
        }));
        Ok(state)
    }

    fn weasel_row(payload: &CapabilitiesResponse) -> anyhow::Result<&CapabilityRow> {
        payload
            .capabilities
            .iter()
            .find(|row| row.id == "weasel_intercept")
            .ok_or_else(|| anyhow::anyhow!("capabilities response must carry a weasel row"))
    }

    #[tokio::test]
    async fn weasel_active_when_the_live_overlay_holds_words_and_boot_config_holds_none()
    -> anyhow::Result<()> {
        let state = state_with_live_overlay(&["pre-existing"]).await?;

        let Json(payload) = capabilities(State(state)).await;
        let row = weasel_row(&payload)?;

        assert_eq!(row.state, CapabilityState::Active, "{}", row.detail);
        assert!(row.detail.contains("1 forbidden word"), "{}", row.detail);
        assert!(row.detail.contains("non-streamed"), "{}", row.detail);
        Ok(())
    }

    #[tokio::test]
    async fn weasel_inert_when_the_live_overlay_holds_no_words() -> anyhow::Result<()> {
        let state = state_with_live_overlay(&[]).await?;

        let Json(payload) = capabilities(State(state)).await;
        let row = weasel_row(&payload)?;

        assert_eq!(row.state, CapabilityState::Inert);
        assert!(!row.fix.is_empty());
        Ok(())
    }
}

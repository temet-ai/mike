use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn service() -> RerankService {
    RerankService::new(&RerankConfig::default())
}

#[test]
fn apply_setting_updates_every_rerank_key() {
    let service = service();

    assert!(service.apply_setting("rerank.enabled", &json!(true)));
    assert!(service.apply_setting("rerank.url", &json!("http://omit:11435/")));
    assert!(service.apply_setting("rerank.model", &json!("qwen3-reranker")));
    assert!(service.apply_setting("rerank.timeout_ms", &json!(900)));
    assert!(service.apply_setting("rerank.candidates", &json!(80)));
    assert!(service.apply_setting("rerank.tuning.enabled", &json!(true)));

    let config = service.current_config();
    assert!(config.enabled);
    assert_eq!(config.url, "http://omit:11435");
    assert_eq!(config.model, "qwen3-reranker");
    assert_eq!(config.timeout_ms, 900);
    assert_eq!(config.candidates, 80);
    assert!(config.tuning_enabled);
}

#[test]
fn apply_setting_with_the_wrong_value_type_changes_nothing() {
    let service = service();

    assert!(!service.apply_setting("rerank.enabled", &json!("yes")));
    assert!(!service.apply_setting("rerank.timeout_ms", &json!("slow")));
    assert!(!service.apply_setting("rerank.url", &json!(5)));

    assert_eq!(service.current_config(), RerankConfig::default());
}

#[test]
fn apply_setting_for_an_unrelated_key_returns_false() {
    assert!(!service().apply_setting("embedding.enabled", &json!(true)));
}

#[test]
fn apply_setting_floors_timeout_and_candidates_at_one() {
    let service = service();

    assert!(service.apply_setting("rerank.timeout_ms", &json!(0)));
    assert!(service.apply_setting("rerank.candidates", &json!(0)));

    let config = service.current_config();
    assert_eq!(config.timeout_ms, 1);
    assert_eq!(config.candidates, 1);
}

#[test]
fn is_enabled_follows_a_live_enabled_change() {
    let service = service();
    assert!(!service.is_enabled());

    service.apply_setting("rerank.enabled", &json!(true));
    assert!(service.is_enabled());

    service.apply_setting("rerank.enabled", &json!(false));
    assert!(!service.is_enabled());
}

#[tokio::test]
async fn apply_setting_for_a_new_url_clears_the_open_breaker() -> anyhow::Result<()> {
    let dead = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&dead)
        .await;
    let service = RerankService::new(&RerankConfig {
        enabled: true,
        url: dead.uri(),
        ..RerankConfig::default()
    });
    let documents = vec!["a".to_owned(), "b".to_owned()];
    for _ in 0..3 {
        let _ = service.rerank("q", &documents, |_| true).await;
    }
    assert!(service.status().breaker_open);

    service.apply_setting("rerank.url", &json!("http://127.0.0.1:1"));

    let status = service.status();
    assert!(!status.breaker_open);
    assert_eq!(status.phase, RerankPhase::Idle);
    assert!(status.last_error.is_none());
    Ok(())
}

#[tokio::test]
async fn apply_setting_enabled_takes_effect_on_the_next_rerank_call() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"index": 0, "relevance_score": 1.0}]
        })))
        .mount(&server)
        .await;
    let service = RerankService::new(&RerankConfig {
        url: server.uri(),
        ..RerankConfig::default()
    });
    let documents = vec!["a".to_owned()];
    assert_eq!(
        service.rerank("q", &documents, |_| true).await,
        Err(RerankError::Disabled)
    );

    service.apply_setting("rerank.enabled", &json!(true));

    let scores = service.rerank("q", &documents, |_| true).await?;
    assert_eq!(scores.len(), 1);
    Ok(())
}

#[test]
fn apply_setting_updates_the_ranking_config_and_leaves_the_rerank_config_alone() {
    let service = service();

    assert!(service.apply_setting("search.ranking.lexical_weight", &json!(2.0)));
    assert!(service.apply_setting("search.ranking.kind_weights", &json!({"note": 0.5})));
    assert!(!service.apply_setting("search.ranking.lexical_weight", &json!("heavy")));

    let ranking = service.ranking();
    assert_eq!(ranking.lexical_weight, 2.0);
    assert_eq!(ranking.kind_weight("note"), 0.5);
    assert_eq!(service.current_config(), RerankConfig::default());
}

#[test]
fn apply_stored_setting_for_a_ranking_key_reports_only_a_real_change() {
    let service = service();

    assert!(!service.apply_stored_setting("search.ranking.semantic_weight", &json!(1.0)));
    assert!(service.apply_stored_setting("search.ranking.semantic_weight", &json!(1.5)));
    assert!(!service.apply_stored_setting("search.ranking.semantic_weight", &json!(1.5)));
    assert_eq!(service.ranking().semantic_weight, 1.5);
}

#[test]
fn new_search_settings_default_to_the_compiled_values_and_apply_live() {
    let service = service();
    let defaults = service.current_config();
    assert_eq!(defaults.cold_timeout_ms, 6_000);
    assert_eq!(defaults.min_query_words, 1);

    assert!(service.apply_setting("rerank.cold_timeout_ms", &json!(9_000)));
    assert!(service.apply_setting("rerank.min_query_words", &json!(3)));
    assert!(!service.apply_setting("rerank.min_query_words", &json!("three")));

    let config = service.current_config();
    assert_eq!(config.cold_timeout_ms, 9_000);
    assert_eq!(config.min_query_words, 3);
}

#[test]
fn new_search_settings_accept_values_in_range_and_refuse_values_out_of_range() {
    use crate::settings::validate_setting_value;
    for (key, value) in [
        ("rerank.cold_timeout_ms", 1_000),
        ("rerank.cold_timeout_ms", 30_000),
        ("rerank.min_query_words", 1),
        ("rerank.min_query_words", 20),
    ] {
        let outcome = validate_setting_value(key, json!(value));
        assert!(outcome.is_ok(), "{key}={value}: {outcome:?}");
    }
    for (key, value) in [
        ("rerank.cold_timeout_ms", 999),
        ("rerank.cold_timeout_ms", 30_001),
        ("rerank.min_query_words", 0),
        ("rerank.min_query_words", 21),
    ] {
        let outcome = validate_setting_value(key, json!(value));
        assert!(outcome.is_err(), "{key}={value} must be refused");
    }
}

#[test]
fn new_search_settings_appear_in_the_settings_metadata_with_the_compiled_defaults()
-> anyhow::Result<()> {
    let config = RerankConfig::default();
    for (key, default) in [
        ("rerank.cold_timeout_ms", json!(config.cold_timeout_ms)),
        ("rerank.min_query_words", json!(config.min_query_words)),
    ] {
        let found = crate::settings::SETTINGS_META
            .iter()
            .find(|m| m.key == key)
            .ok_or_else(|| anyhow::anyhow!("{key} is not in SETTINGS_META"))?;
        assert_eq!(found.group, "Reranker");
        assert_eq!(found.default, default);
    }
    Ok(())
}

use chrono::{TimeZone, Utc};

use super::SemanticError;
use super::egress::{EgressState, allow, egress_state, is_loopback_url};
use super::embedder::{
    CanaryVectors, EmbedderShape, cosine, embedder_key, same_embedder, verify_passes,
};
use super::floors::{Floor, FloorSet, best_allowed, floor_status, percentile, promotion_refused};
use crate::analysis::embeddings::{EmbedError, EmbeddingConfig};

fn cfg(url: &str) -> EmbeddingConfig {
    EmbeddingConfig {
        enabled: true,
        url: url.to_owned(),
        ..Default::default()
    }
}

#[test]
fn loopback_hosts_recognised_for_ipv4_ipv6_and_localhost() {
    assert!(is_loopback_url("http://127.0.0.1:11434/v1"));
    assert!(is_loopback_url("http://127.3.4.5:7997"));
    assert!(is_loopback_url("http://[::1]:7997"));
    assert!(is_loopback_url("http://localhost:7997"));
    assert!(!is_loopback_url("http://192.168.1.20:11434"));
}

#[test]
fn remote_host_blocks_embedding_unless_allowed() {
    let remote = cfg("https://embeddings.example.com/v1");
    assert_eq!(egress_state(&remote, false), EgressState::RemoteBlocked);
    assert!(!allow(&remote, false));
    assert_eq!(egress_state(&remote, true), EgressState::RemoteAllowed);
    assert!(allow(&cfg("http://127.0.0.1:1"), false));
}

#[test]
fn unparseable_url_is_treated_as_remote() {
    assert!(!is_loopback_url("not a url"));
    assert!(!allow(&cfg("::::"), false));
}

#[test]
fn embedder_key_is_plain_text_with_since_time() {
    let since = Utc.with_ymd_and_hms(2026, 10, 2, 9, 14, 7).single();
    let key = since.map(|s| embedder_key("nomic-embed-text-v1.5", 768, 1024, s));
    assert_eq!(
        key.as_deref(),
        Some("nomic-embed-text-v1.5|768|1024|doc-prefix-v1|since:2026-10-02T09:14:07Z")
    );
}

fn shape<'a>(canaries: &'a CanaryVectors, model: &'a str) -> EmbedderShape<'a> {
    EmbedderShape {
        model,
        dim: 3,
        chunk_max_chars: 1024,
        prefix_version: "doc-prefix-v1",
        canaries,
    }
}

#[test]
fn same_canaries_under_new_url_keep_the_embedder() {
    let a = CanaryVectors::new(vec![vec![1.0, 0.0, 0.0]; 8]);
    let b = CanaryVectors::new(vec![vec![1.0, 0.0, 0.0]; 8]);
    assert!(same_embedder(&shape(&a, "m"), &shape(&b, "m")));
}

#[test]
fn canary_cosine_below_threshold_creates_new_embedder() {
    let a = CanaryVectors::new(vec![vec![1.0, 0.0, 0.0]; 8]);
    let mut shifted = vec![vec![1.0, 0.0, 0.0]; 8];
    shifted[5] = vec![0.96, 0.28, 0.0];
    let b = CanaryVectors::new(shifted);
    assert!(!same_embedder(&shape(&a, "m"), &shape(&b, "m")));
    assert!(!same_embedder(&shape(&a, "m"), &shape(&a, "other-model")));
}

#[test]
fn verify_refuses_window_when_any_sample_differs() {
    assert!(verify_passes(&[1.0, 0.99995]));
    assert!(!verify_passes(&[1.0, 0.95]));
    assert!(!verify_passes(&[]));
    assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-12);
}

fn floors() -> FloorSet {
    let mut set = FloorSet::default();
    set.by_pair
        .insert("prompt:corr".to_owned(), Floor { id: 1, floor: 0.80 });
    set.by_pair
        .insert("ending:corr".to_owned(), Floor { id: 2, floor: 0.76 });
    set
}

#[test]
fn score_is_similarity_minus_pair_floor() -> anyhow::Result<()> {
    let s = floors().score("prompt:corr", 0.85)?;
    assert!((s.score - 0.05).abs() < 1e-9);
    assert_eq!(s.floor_id, 1);
    Ok(())
}

#[test]
fn item_score_takes_best_allowed_pair_and_a_missing_floor_is_an_error() {
    let set = floors();
    let best = best_allowed(
        &set,
        &[
            ("prompt:corr", 0.85),
            ("ending:corr", 0.84),
            ("ending:ending", 0.99),
        ],
        &["prompt:corr", "ending:corr"],
    );
    assert_eq!(best.map(|(p, _)| p).as_deref(), Some("ending:corr"));
    assert!(set.score("ending:ending", 0.99).is_err());
}

#[test]
fn floor_matches_reference_percentile_on_fixed_sample() {
    let sorted: Vec<f64> = (0..=1000).map(|i| f64::from(i) / 1000.0).collect();
    assert_eq!(percentile(&sorted, 0.999), Some(0.999));
    assert_eq!(percentile(&[0.2, 0.4], 0.5), Some(0.30000000000000004));
    assert_eq!(percentile(&[], 0.5), None);
}

#[test]
fn insufficient_pair_never_enters_force_and_recompute_is_monitoring() {
    assert_eq!(floor_status(9_999, true), "insufficient");
    assert_eq!(floor_status(40_000, true), "in_force");
    assert_eq!(floor_status(40_000, false), "monitoring");
}

#[test]
fn promote_refused_while_llm_facing_consumer_uses_pair() {
    assert!(promotion_refused("prompt:corr", &["prompt:corr"], false));
    assert!(!promotion_refused("prompt:corr", &["prompt:corr"], true));
    assert!(!promotion_refused("corr:corr", &["prompt:corr"], false));
}

#[test]
fn only_the_disabled_embedding_error_counts_as_configured_off() {
    assert!(SemanticError::Embed(EmbedError::Disabled).is_embedding_disabled());
    assert!(!SemanticError::Embed(EmbedError::Http("down".to_owned())).is_embedding_disabled());
    assert!(!SemanticError::Disabled.is_embedding_disabled());
    assert!(!SemanticError::NoEmbedder.is_embedding_disabled());
}

#[test]
fn a_disabled_embedder_does_not_trip_the_breaker_and_a_failing_one_does() {
    assert!(!SemanticError::Embed(EmbedError::Disabled).trips_breaker());
    assert!(SemanticError::Embed(EmbedError::Http("down".to_owned())).trips_breaker());
    assert!(!SemanticError::NoEmbedder.trips_breaker());
}

#[test]
fn floors_due_waits_a_week_after_floors_were_computed_even_across_restarts() {
    let two_days_ago = Some(Utc::now() - chrono::Duration::days(2));
    let eight_days_ago = Some(Utc::now() - chrono::Duration::days(8));
    assert!(!super::worker::floors_due(true, two_days_ago, None));
    assert!(super::worker::floors_due(true, eight_days_ago, None));
    assert!(super::worker::floors_due(true, None, None));
}

#[test]
fn floors_due_retries_a_key_without_floors_daily_and_not_every_tick() {
    let two_days_ago = Some(Utc::now() - chrono::Duration::days(2));
    let an_hour_ago = Some(Utc::now() - chrono::Duration::hours(1));
    assert!(super::worker::floors_due(false, two_days_ago, None));
    assert!(!super::worker::floors_due(false, an_hour_ago, None));
    assert!(!super::worker::floors_due(false, two_days_ago, an_hour_ago));
}

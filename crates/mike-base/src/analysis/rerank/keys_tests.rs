use serde_json::json;

use super::{RerankConfig, assign_rerank_key};
use crate::settings::SETTINGS_META;

fn rerank_keys() -> Vec<&'static str> {
    SETTINGS_META
        .iter()
        .map(|meta| meta.key)
        .filter(|key| key.starts_with("rerank."))
        .collect()
}

#[test]
fn every_registered_rerank_key_assigns_its_own_default() {
    for key in rerank_keys() {
        let meta = SETTINGS_META.iter().find(|m| m.key == key);
        let default = meta.map(|m| m.default.clone()).unwrap_or_default();
        let mut config = RerankConfig::default();

        assert!(assign_rerank_key(&mut config, key, &default), "{key}");
        assert_eq!(config, RerankConfig::default(), "{key} default differs");
    }
}

#[test]
fn an_unknown_rerank_key_assigns_nothing() {
    let mut config = RerankConfig::default();

    assert!(!assign_rerank_key(&mut config, "rerank.nope", &json!(1)));
    assert_eq!(config, RerankConfig::default());
}

#[test]
fn nightly_hours_out_of_range_clamp_to_a_real_hour() {
    let mut config = RerankConfig::default();

    assign_rerank_key(&mut config, "rerank.nightly.start_hour_utc", &json!(30));
    assign_rerank_key(&mut config, "rerank.nightly.end_hour_utc", &json!(-2));

    assert_eq!(config.nightly.start_hour_utc, 23);
    assert_eq!(config.nightly.end_hour_utc, 0);
}

#[test]
fn nightly_window_wraps_past_midnight_and_an_empty_window_never_runs() {
    let mut nightly = RerankConfig::default().nightly;
    let (start, end) = (nightly.start_hour_utc, nightly.end_hour_utc);

    assert!(nightly.in_window(start) && nightly.in_window(end.saturating_sub(1)));
    assert!(!nightly.in_window(end));
    nightly.end_hour_utc = start;
    assert!((0..24).all(|hour| !nightly.in_window(hour)));
}

use std::time::Duration;

use super::*;

fn fresh(scores: &[(usize, f32)]) -> Vec<RerankScore> {
    scores
        .iter()
        .map(|(index, score)| RerankScore {
            index: *index,
            score: *score,
        })
        .collect()
}

fn stored(cache: &ScoreCache, query: &str, documents: &[&str], scores: &[(usize, f32)]) {
    let lookup = cache.lookup("model", query, documents);
    let missing = lookup.missing();
    cache.store(&lookup, &missing, &fresh(scores));
}

#[test]
fn lookup_of_a_stored_pair_finds_its_score() {
    let cache = ScoreCache::new();
    stored(&cache, "query", &["a", "b"], &[(0, 0.25), (1, 0.75)]);

    let lookup = cache.lookup("model", "query", &["b", "c", "a"]);

    assert_eq!(lookup.cached(), 2);
    assert_eq!(lookup.missing(), vec![1]);
}

#[test]
fn lookup_under_another_query_or_model_finds_nothing() {
    let cache = ScoreCache::new();
    stored(&cache, "query", &["a"], &[(0, 0.5)]);

    assert_eq!(cache.lookup("model", "other", &["a"]).cached(), 0);
    assert_eq!(cache.lookup("other", "query", &["a"]).cached(), 0);
}

#[test]
fn lookup_after_the_lifetime_finds_nothing() {
    let cache = ScoreCache::with_limits(10, Duration::from_secs(60));
    stored(&cache, "query", &["a"], &[(0, 0.5)]);

    cache.advance(Duration::from_secs(59));
    assert_eq!(cache.lookup("model", "query", &["a"]).cached(), 1);

    cache.advance(Duration::from_secs(2));
    assert_eq!(cache.lookup("model", "query", &["a"]).cached(), 0);
}

#[test]
fn store_at_capacity_stays_within_capacity_and_keeps_the_newest() {
    let cache = ScoreCache::with_limits(20, Duration::from_secs(3600));
    for round in 0..100u32 {
        let document = format!("document {round}");
        stored(&cache, "query", &[document.as_str()], &[(0, 0.5)]);
        cache.advance(Duration::from_millis(5));
    }

    assert!(cache.len() <= 20, "{} entries", cache.len());
    assert_eq!(cache.lookup("model", "query", &["document 99"]).cached(), 1);
    assert_eq!(cache.lookup("model", "query", &["document 0"]).cached(), 0);
}

#[test]
fn store_at_capacity_drops_expired_entries_first() {
    let cache = ScoreCache::with_limits(3, Duration::from_secs(60));
    stored(
        &cache,
        "query",
        &["a", "b", "c"],
        &[(0, 0.1), (1, 0.2), (2, 0.3)],
    );
    cache.advance(Duration::from_secs(61));

    stored(&cache, "query", &["d"], &[(0, 0.4)]);

    assert_eq!(cache.len(), 1);
}

#[test]
fn merged_returns_cached_and_fresh_scores_best_first_with_original_indices() {
    let cache = ScoreCache::new();
    stored(&cache, "query", &["a", "c"], &[(0, 0.5), (1, 0.1)]);
    let lookup = cache.lookup("model", "query", &["a", "b", "c", "d"]);
    let missing = lookup.missing();

    let merged = lookup.merged(fresh(&[(0, 0.9), (1, 0.3)]), &missing);

    let order: Vec<(usize, f32)> = merged.iter().map(|s| (s.index, s.score)).collect();
    assert_eq!(order, vec![(1, 0.9), (0, 0.5), (3, 0.3), (2, 0.1)]);
}

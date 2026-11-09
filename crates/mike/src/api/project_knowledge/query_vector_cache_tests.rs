use super::query_vector_cache::QueryVectorCache;

#[test]
fn get_after_put_returns_the_stored_vector() {
    let cache = QueryVectorCache::with_capacity(4);
    cache.put("model-a", "how are leases renewed", vec![0.5, 0.25]);
    assert_eq!(
        cache.get("model-a", "how are leases renewed"),
        Some(vec![0.5, 0.25])
    );
}

#[test]
fn get_for_another_model_key_misses() {
    let cache = QueryVectorCache::with_capacity(4);
    cache.put("model-a", "query", vec![1.0]);
    assert_eq!(cache.get("model-b", "query"), None);
}

#[test]
fn get_for_another_query_text_misses() {
    let cache = QueryVectorCache::with_capacity(4);
    cache.put("model-a", "query", vec![1.0]);
    assert_eq!(cache.get("model-a", "query "), None);
}

#[test]
fn put_beyond_capacity_evicts_the_least_recently_used_entry() {
    let cache = QueryVectorCache::with_capacity(2);
    cache.put("m", "first", vec![1.0]);
    cache.put("m", "second", vec![2.0]);
    assert!(cache.get("m", "first").is_some());
    cache.put("m", "third", vec![3.0]);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.get("m", "second"), None);
    assert!(cache.get("m", "first").is_some());
    assert!(cache.get("m", "third").is_some());
}

#[test]
fn put_for_an_existing_key_replaces_the_vector_without_growing() {
    let cache = QueryVectorCache::with_capacity(2);
    cache.put("m", "q", vec![1.0]);
    cache.put("m", "q", vec![2.0]);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.get("m", "q"), Some(vec![2.0]));
}

use figment::Figment;
use serde::de::DeserializeOwned;

use super::*;
use crate::environment::MapEnvironment;

fn extracted<T: DeserializeOwned>(pairs: &[(&str, &str)], key: &str) -> anyhow::Result<T> {
    let environment = MapEnvironment::new(pairs);
    Ok(merge_prefixed(Figment::new(), &environment).extract_inner(key)?)
}

#[test]
fn field_key_for_a_prefixed_name_is_the_lowercase_remainder() {
    assert_eq!(
        field_key("MIKE_MAX_REQUESTS").as_deref(),
        Some("max_requests")
    );
    assert_eq!(
        field_key("MIKE_DB_POOL_CAPTURE_SIZE").as_deref(),
        Some("db_pool_capture_size")
    );
}

#[test]
fn field_key_for_the_two_renamed_addresses_is_the_proxy_field_name() {
    assert_eq!(
        field_key("MIKE_LISTEN_ADDR").as_deref(),
        Some("proxy_listen_addr")
    );
    assert_eq!(
        field_key("MIKE_INSPECT_ADDR").as_deref(),
        Some("inspect_listen_addr")
    );
    assert_eq!(
        field_key("mike_listen_addr").as_deref(),
        Some("proxy_listen_addr")
    );
}

#[test]
fn field_key_for_a_double_underscore_nests_with_a_dot() {
    assert_eq!(
        field_key("MIKE_EMBEDDING__CHUNK_MAX_CHARS").as_deref(),
        Some("embedding.chunk_max_chars")
    );
    assert_eq!(
        field_key("MIKE_STEERER__OPENROUTER_API_KEY").as_deref(),
        Some("steerer.openrouter_api_key")
    );
}

#[test]
fn field_key_for_a_name_without_the_prefix_is_none() {
    for name in ["PATH", "XDG_CONFIG_HOME", "OPENROUTER_API_KEY", "MIK_X", ""] {
        assert_eq!(field_key(name), None, "{name}");
    }
}

#[test]
fn field_key_with_an_empty_path_segment_is_none() {
    for name in ["MIKE_", "MIKE_A__", "MIKE___A", "MIKE_A____B"] {
        assert_eq!(field_key(name), None, "{name}");
    }
}

#[test]
fn field_key_ignores_whitespace_around_the_name() {
    assert_eq!(field_key(" MIKE_MAX_AGE ").as_deref(), Some("max_age"));
}

#[test]
fn layer_value_that_reads_as_a_number_is_a_number() -> anyhow::Result<()> {
    assert_eq!(
        extracted::<u64>(&[("MIKE_MAX_REQUESTS", "42")], "max_requests")?,
        42
    );
    Ok(())
}

#[test]
fn layer_value_that_reads_as_a_boolean_is_a_boolean() -> anyhow::Result<()> {
    assert!(extracted::<bool>(
        &[("MIKE_PERSIST_ENABLED", "true")],
        "persist_enabled"
    )?);
    Ok(())
}

#[test]
fn layer_value_that_reads_as_a_list_is_a_list() -> anyhow::Result<()> {
    assert_eq!(
        extracted::<Vec<String>>(&[("MIKE_ORIGINS", "[\"a\", \"b\"]")], "origins")?,
        vec!["a".to_owned(), "b".to_owned()]
    );
    Ok(())
}

#[test]
fn layer_value_that_is_plain_text_stays_text() -> anyhow::Result<()> {
    assert_eq!(
        extracted::<String>(&[("MIKE_MAX_AGE", "2h")], "max_age")?,
        "2h"
    );
    Ok(())
}

#[test]
fn layer_nested_names_merge_into_one_table() -> anyhow::Result<()> {
    let pairs = [
        ("MIKE_EMBEDDING__URL", "http://127.0.0.1:1"),
        ("MIKE_EMBEDDING__MODEL", "m"),
    ];
    assert_eq!(
        extracted::<String>(&pairs, "embedding.url")?,
        "http://127.0.0.1:1"
    );
    assert_eq!(extracted::<String>(&pairs, "embedding.model")?, "m");
    Ok(())
}

#[test]
fn layer_skips_variables_outside_the_prefix() {
    let environment = MapEnvironment::new(&[("PATH", "/bin"), ("MIKE_MAX_AGE", "1h")]);
    let keys: Vec<String> = layer_entries(&environment)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys, vec!["max_age".to_owned()]);
}

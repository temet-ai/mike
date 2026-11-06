use crate::common;

use super::support::{
    poll_detail_for_complete, post_and_get_id, sse_body_with_trigger, start_sse_mock,
};
use mike::analysis::AnalysisConfig;
use serde_json::Value;

#[tokio::test]
async fn response_with_trigger_text_produces_diagnostics_on_detail() -> anyhow::Result<()> {
    let trigger = "This is a pragmatic and robust implementation that is production-ready for now";
    let mock_server = start_sse_mock(sse_body_with_trigger(trigger)).await;

    let (proxy_url, inspect_url) = common::start_proxy(&mock_server.uri()).await?;
    let client = reqwest::Client::new();

    let id = post_and_get_id(&client, &proxy_url, &inspect_url).await?;
    let detail = poll_detail_for_complete(&client, &inspect_url, id, true).await?;

    let diagnostics = detail["diagnostics"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("diagnostics must be a JSON array: {detail}"))?;
    assert!(
        !diagnostics.is_empty(),
        "analyzer should have produced at least one diagnostic: {detail}"
    );

    let mut by_cat: std::collections::BTreeMap<String, &Value> = Default::default();
    for diag in diagnostics {
        let cat = diag["category"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("category must be a string: {diag}"))?
            .to_string();
        by_cat.entry(cat).or_insert(diag);
    }

    assert!(
        by_cat.contains_key("justifying_shortcuts"),
        "justifying_shortcuts missing: {diagnostics:?}"
    );
    let over = by_cat
        .get("overselling")
        .ok_or_else(|| anyhow::anyhow!("overselling missing: {diagnostics:?}"))?;

    let over_other_hits = over["other_hits"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("other_hits must be u64: {over}"))?;
    assert!(
        over_other_hits >= 1,
        "overselling finding must report at least one extra hit (got {over_other_hits})"
    );

    for diag in diagnostics {
        let advice = diag["advice"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("advice must be a string: {diag}"))?;
        assert!(!advice.is_empty(), "advice must not be empty: {diag}");
        let matched = diag["matched_text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("matched_text must be a string: {diag}"))?;
        assert!(
            !matched.is_empty(),
            "matched_text must not be empty: {diag}"
        );
    }

    let stats: Value = client
        .get(format!("{inspect_url}/api/diagnostics/stats"))
        .send()
        .await?
        .json()
        .await?;
    let by_cat_stats = stats["totals_by_category"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("totals_by_category must be an object: {stats}"))?;
    assert!(
        by_cat_stats
            .get("justifying_shortcuts")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 1,
        "stats must report justifying_shortcuts >= 1: {stats}"
    );
    assert!(
        by_cat_stats
            .get("overselling")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 1,
        "stats must report overselling >= 1: {stats}"
    );
    assert!(
        stats["last_24h"].as_u64().unwrap_or(0) >= 1,
        "last_24h must see the fresh findings: {stats}"
    );
    Ok(())
}

#[tokio::test]
async fn disabled_category_suppresses_findings_but_allows_others() -> anyhow::Result<()> {
    let analysis: AnalysisConfig = toml::from_str(
        r#"
[categories.overselling]
enabled = false
"#,
    )?;

    let trigger = "This is a pragmatic and robust implementation that is production-ready for now";
    let mock_server = start_sse_mock(sse_body_with_trigger(trigger)).await;

    let (proxy_url, inspect_url) =
        common::start_proxy_with_analysis(&mock_server.uri(), analysis).await?;
    let client = reqwest::Client::new();

    let id = post_and_get_id(&client, &proxy_url, &inspect_url).await?;
    let detail = poll_detail_for_complete(&client, &inspect_url, id, true).await?;

    let diagnostics = detail["diagnostics"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("diagnostics must be a JSON array: {detail}"))?;

    let overselling_present = diagnostics
        .iter()
        .any(|d| d["category"].as_str() == Some("overselling"));
    assert!(
        !overselling_present,
        "overselling was disabled via config but still fired: {diagnostics:?}"
    );
    let js_present = diagnostics
        .iter()
        .any(|d| d["category"].as_str() == Some("justifying_shortcuts"));
    assert!(
        js_present,
        "justifying_shortcuts must still fire when only overselling is disabled: {diagnostics:?}"
    );
    Ok(())
}

#[tokio::test]
async fn false_positive_filter_suppresses_sentence_level_match() -> anyhow::Result<()> {
    let analysis: AnalysisConfig = toml::from_str(
        r#"
[categories.justifying_shortcuts]
false_positive_filters = ["pragmatic approach"]
"#,
    )?;

    let suppressed_mock = start_sse_mock(sse_body_with_trigger(
        "Using a pragmatic approach to the refactor.",
    ))
    .await;
    let (proxy_url, inspect_url) =
        common::start_proxy_with_analysis(&suppressed_mock.uri(), analysis.clone()).await?;
    let client = reqwest::Client::new();
    let id = post_and_get_id(&client, &proxy_url, &inspect_url).await?;
    let detail = poll_detail_for_complete(&client, &inspect_url, id, false).await?;
    let diagnostics = detail["diagnostics"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("diagnostics must be a JSON array: {detail}"))?;
    let js_present = diagnostics
        .iter()
        .any(|d| d["category"].as_str() == Some("justifying_shortcuts"));
    assert!(
        !js_present,
        "sentence `pragmatic approach` must be suppressed: {diagnostics:?}"
    );

    let positive_mock = start_sse_mock(sse_body_with_trigger("This is pragmatic.")).await;
    let (proxy_url2, inspect_url2) =
        common::start_proxy_with_analysis(&positive_mock.uri(), analysis).await?;
    let id2 = post_and_get_id(&client, &proxy_url2, &inspect_url2).await?;
    let detail2 = poll_detail_for_complete(&client, &inspect_url2, id2, true).await?;
    let diagnostics2 = detail2["diagnostics"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("diagnostics2 must be an array: {detail2}"))?;
    let js_present2 = diagnostics2
        .iter()
        .any(|d| d["category"].as_str() == Some("justifying_shortcuts"));
    assert!(
        js_present2,
        "`this is pragmatic` must still fire when the filter targets a different sentence: {diagnostics2:?}"
    );
    Ok(())
}

#[tokio::test]
async fn per_pattern_advice_override_from_toml_config() -> anyhow::Result<()> {
    let analysis: AnalysisConfig = toml::from_str(
        r#"
[patterns.robust]
advice = "custom overselling advice"
"#,
    )?;

    let trigger = "This is a robust and pragmatic implementation";
    let mock_server = start_sse_mock(sse_body_with_trigger(trigger)).await;
    let (proxy_url, inspect_url) =
        common::start_proxy_with_analysis(&mock_server.uri(), analysis).await?;
    let client = reqwest::Client::new();

    let id = post_and_get_id(&client, &proxy_url, &inspect_url).await?;
    let detail = poll_detail_for_complete(&client, &inspect_url, id, true).await?;

    let diagnostics = detail["diagnostics"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("diagnostics must be a JSON array: {detail}"))?;

    let over = diagnostics
        .iter()
        .find(|d| d["category"].as_str() == Some("overselling"))
        .ok_or_else(|| anyhow::anyhow!("overselling must fire: {diagnostics:?}"))?;
    assert_eq!(
        over["advice"].as_str(),
        Some("custom overselling advice"),
        "advice override must replace built-in text end-to-end via TOML"
    );

    for diag in diagnostics {
        let category = diag["category"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("category must be a string: {diag}"))?;
        let advice = diag["advice"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("advice must be a string: {diag}"))?;
        if category == "overselling" {
            assert_eq!(
                advice, "custom overselling advice",
                "overselling must carry the override: {diag}"
            );
        } else if category == "justifying_shortcuts" {
            assert_eq!(
                advice, "not a licence to reduce scope",
                "justifying_shortcuts must keep its built-in advice: {diag}"
            );
        } else {
            anyhow::bail!("unexpected category fired on `robust and pragmatic` trigger: {diag}");
        }
    }
    Ok(())
}

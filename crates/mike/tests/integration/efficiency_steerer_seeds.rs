use crate::db_helpers;

use serde_json::Value;

const SEED_NAMES: [&str; 5] = [
    "no-full-spill-read",
    "no-cat-in-bash",
    "ast-grep-over-grep",
    "monitor-over-sleep",
    "tee-then-slice-gates",
];

const AUTO_ENROLLED: [&str; 3] = [
    "ast-grep-over-grep",
    "monitor-over-sleep",
    "tee-then-slice-gates",
];

#[tokio::test]
async fn efficiency_steerer_seeds_visible_via_get_api_steerers() -> anyhow::Result<()> {
    let upstream = wiremock::MockServer::start().await;

    let Some(env) = db_helpers::start_proxy_with_shared_db(&upstream.uri()).await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset — skipping \
             efficiency_steerer_seeds_visible_via_get_api_steerers"
        );
        return Ok(());
    };

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/steerers", env.inspect_url))
        .send()
        .await?;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let steerers: Vec<Value> = resp.json().await?;

    for name in SEED_NAMES {
        let row = steerers
            .iter()
            .find(|s| s["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("seed steerer {name:?} not found in GET /api/steerers"));

        assert_eq!(row["kind"].as_str(), Some("refocus"), "{name}: kind");
        assert_eq!(
            row["trigger_point"].as_str(),
            Some("response"),
            "{name}: trigger_point"
        );
        assert_eq!(
            row["invocation"].as_str(),
            Some("pattern"),
            "{name}: invocation"
        );
        assert!(
            row["pattern"].as_str().is_some_and(|p| !p.is_empty()),
            "{name}: pattern must be non-empty"
        );
        assert_eq!(row["enabled"].as_bool(), Some(true), "{name}: enabled");
        assert_eq!(
            row["auto_enrol"].as_bool(),
            Some(AUTO_ENROLLED.contains(&name)),
            "{name}: auto_enrol"
        );
    }

    for name in SEED_NAMES {
        let count = steerers
            .iter()
            .filter(|s| s["name"].as_str() == Some(name))
            .count();
        assert_eq!(count, 1, "{name}: expected exactly one row, found {count}");
    }

    env.shutdown.cancel();
    env.test_db.cleanup().await.ok();
    Ok(())
}

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde_json::{Value, json};

use super::conversation_tests::read;
use super::followups::{
    FollowupLimitsBody, FollowupParams, get_followup_limits, list_followups, patch_followup_limits,
    patch_followup_limits_as_operator, validate_followup_limits,
};
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

struct Env {
    db: TestDbHandle,
    state: AppState,
}

impl Env {
    async fn boot() -> anyhow::Result<Self> {
        let db = acquire_ephemeral_db()
            .await?
            .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
        let config = ProxyConfig::try_parse_from([
            "mike",
            "--upstream-url",
            "https://api.anthropic.com",
            "--proxy-listen-addr",
            "127.0.0.1:0",
            "--inspect-listen-addr",
            "127.0.0.1:0",
            "--persist-enabled",
            "1",
            "--database-url",
            &db.database_url,
        ])?;
        let state = AppState::new(config).await?;
        Ok(Self { db, state })
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.state.shutdown().cancel();
        self.db.cleanup().await
    }

    async fn followup(&self, part: &str, state: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO wright.followups (kind, owning_part, title, body, state, issue_number,
                                           closed_at)
             VALUES ('hardening', $1, 'title', 'body', $2,
                     CASE WHEN $2 = 'open' THEN NULL ELSE 700 END,
                     CASE WHEN $2 = 'fixed' THEN now() END)
             RETURNING id",
        )
        .bind(part)
        .bind(state)
        .fetch_one(
            self.state
                .persist()
                .ok_or_else(|| anyhow::anyhow!("persistence is required"))?
                .pool(),
        )
        .await?)
    }

    async fn list(&self, params: Value) -> anyhow::Result<(StatusCode, Value)> {
        let params: FollowupParams = serde_json::from_value(params)?;
        match list_followups(State(self.state.clone()), Query(params)).await {
            Ok(Json(value)) => Ok((StatusCode::OK, value)),
            Err(response) => read(response).await,
        }
    }
}

fn ids(page: &Value) -> Vec<i64> {
    page["followups"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["id"].as_i64()).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn followups_list_pages_complete_rows_newest_first_and_filters_by_state() -> anyhow::Result<()>
{
    let env = Env::boot().await?;
    let first = env.followup("a", "open").await?;
    let second = env.followup("b", "filed").await?;
    let third = env.followup("c", "filed").await?;

    let (status, page) = env.list(json!({"limit": 2})).await?;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(ids(&page), vec![third, second]);
    assert_eq!(page["next_before"], second);
    let row = &page["followups"][0];
    for field in [
        "kind",
        "title",
        "state",
        "issue_number",
        "issue_url",
        "fixer_session",
        "fixer_machine",
        "attempts",
        "findings",
        "problems",
    ] {
        assert!(row.get(field).is_some(), "{field} missing from {row}");
    }

    let (_, rest) = env.list(json!({"limit": 2, "before": second})).await?;
    assert_eq!(ids(&rest), vec![first]);
    assert_eq!(rest["next_before"], Value::Null);

    let (_, open) = env.list(json!({"state": "open"})).await?;
    assert_eq!(ids(&open), vec![first]);
    assert_eq!(open["next_before"], Value::Null);
    env.cleanup().await
}

#[tokio::test]
async fn followups_list_refuses_a_bad_limit_before_or_state_naming_the_value() -> anyhow::Result<()>
{
    let env = Env::boot().await?;
    for (params, needle) in [
        (
            json!({"limit": 201}),
            "limit is 201; it must be between 1 and 200",
        ),
        (json!({"limit": 0}), "limit is 0"),
        (json!({"before": 0}), "before is 0"),
        (json!({"state": "done"}), "state is `done`"),
    ] {
        let (status, body) = env.list(params.clone()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{params}");
        assert!(
            body["error"].as_str().is_some_and(|e| e.contains(needle)),
            "{params}: {body}"
        );
    }
    env.cleanup().await
}

fn body(value: Value) -> anyhow::Result<FollowupLimitsBody> {
    Ok(serde_json::from_value(value)?)
}

#[tokio::test]
async fn followup_limits_patch_without_the_operator_token_is_refused_and_changes_nothing()
-> anyhow::Result<()> {
    let env = Env::boot().await?;
    let Json(before) = get_followup_limits(State(env.state.clone()))
        .await
        .map_err(|_| anyhow::anyhow!("limits were not readable"))?;
    let refused = patch_followup_limits_as_operator(
        State(env.state.clone()),
        HeaderMap::new(),
        Json(body(json!({"daily_filed": 7}))?),
    )
    .await;
    let Err(response) = refused else {
        anyhow::bail!("a patch without a bearer was applied");
    };
    assert_eq!(response.into_response().status(), StatusCode::UNAUTHORIZED);
    let Json(after) = get_followup_limits(State(env.state.clone()))
        .await
        .map_err(|_| anyhow::anyhow!("limits were not readable"))?;
    assert_eq!(after["limits"], before["limits"]);
    env.cleanup().await
}

#[tokio::test]
async fn followup_limits_patch_applies_the_named_fields_and_reports_ranges() -> anyhow::Result<()> {
    let env = Env::boot().await?;
    let patched = patch_followup_limits(
        State(env.state.clone()),
        Json(body(json!({
            "daily_filed": 7,
            "fixers": [{"machine": "omit", "host": null, "folder": null}],
        }))?),
    )
    .await;
    let Ok(Json(answer)) = patched else {
        anyhow::bail!("the patch was refused");
    };
    assert_eq!(answer["limits"]["daily_filed"], 7);
    assert_eq!(answer["limits"]["fixers"][0]["machine"], "omit");
    assert_eq!(answer["ranges"]["daily_filed"], json!([0, 20]));
    let Json(read_back) = get_followup_limits(State(env.state.clone()))
        .await
        .map_err(|_| anyhow::anyhow!("limits were not readable"))?;
    assert_eq!(read_back["limits"]["daily_filed"], 7);
    env.cleanup().await
}

#[test]
fn followup_limits_validation_names_field_value_and_range() -> anyhow::Result<()> {
    for (value, needle) in [
        (json!({}), "names no limit"),
        (
            json!({"daily_filed": 21}),
            "daily_filed is 21; it must be between 0 and 20",
        ),
        (
            json!({"max_attempts": 0}),
            "max_attempts is 0; it must be between 1 and 5",
        ),
        (json!({"recurrence_days": 31}), "recurrence_days is 31"),
        (
            json!({"repo": "mike"}),
            "repo is `mike`; it must look like owner/name",
        ),
        (
            json!({"fixers": [{"machine": " ", "host": null, "folder": null}]}),
            "fixers[0]",
        ),
    ] {
        let Err(reason) = validate_followup_limits(body(value.clone())?) else {
            anyhow::bail!("{value} was accepted");
        };
        assert!(reason.contains(needle), "{value}: {reason}");
    }
    let patch =
        validate_followup_limits(body(json!({"repo": "temet-ai/mike", "daily_handoffs": 0}))?);
    assert!(patch.is_ok_and(|p| p.daily_handoffs == Some(0)));
    assert!(serde_json::from_value::<FollowupLimitsBody>(json!({"daily": 1})).is_err());
    Ok(())
}

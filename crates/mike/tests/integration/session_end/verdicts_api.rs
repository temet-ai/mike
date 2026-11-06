use chrono::Utc;
use mike::store::persistence::session_end::SessionVerdictV3Row;
use serde_json::Value;

use super::frustration::serve;
use crate::judge_quality::promotion_support::{bundle, closed_turn, facts};
use crate::turn_facts::{Fixture, fixture};

fn row<'a>(
    f: &Fixture,
    outcome_id: i64,
    mode: &'a str,
    end_turn: i64,
    input: &'a str,
    bundle_id: Option<i64>,
) -> SessionVerdictV3Row<'a> {
    SessionVerdictV3Row {
        session_db_id: f.session_id,
        outcome_id,
        quality_score: 0.3,
        rationale: "the operator gave up on the retry",
        judge_model: "google/gemini-3.5-flash",
        rubric_version: 3,
        mode,
        bundle_id,
        rubric_id: "session-v3-2026-09-24",
        end_turn_id: Some(end_turn),
        end_label: Some("closed_in_frustration"),
        end_source: Some("llm"),
        end_rule: None,
        end_confidence: Some("high"),
        end_evidence: Some("operator: forget it, I'll do it myself"),
        failure: Some("the retry was never tested"),
        failure_tags: vec!["unverified_claim".to_owned()],
        severity: Some("major"),
        slices: Vec::new(),
        as_of: Utc::now(),
        digest_version: "dossier-r1",
        input_text: input,
        llm_output: serde_json::json!({"end_label": "closed_in_frustration"}),
        generation_id: None,
        cost_usd: Some(0.004),
        prompt_tokens: Some(9_000),
        completion_tokens: Some(300),
        latency_ms: Some(2_100),
    }
}

#[tokio::test]
async fn get_session_verdicts_returns_every_mode_whole() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let ending = closed_turn(&f, 31_001).await?;
    facts(&f, ending, "genuine", "none_session_settled").await?;
    let outcome_id: i64 =
        sqlx::query_scalar("SELECT id FROM session_judges.outcomes WHERE code = 'failure'")
            .fetch_one(f.handle.pool())
            .await?;
    let input = "SESSION DIGEST\n".repeat(2_000);
    let live = f
        .handle
        .insert_session_verdict_v3(&row(&f, outcome_id, "live", ending, &input, None))
        .await?;
    let eval_bundle = bundle(&f).await?;
    let eval = f
        .handle
        .insert_session_verdict_v3(&row(
            &f,
            outcome_id,
            "eval",
            ending,
            &input,
            Some(eval_bundle),
        ))
        .await?;
    let uuid: String =
        sqlx::query_scalar("SELECT session_uuid::text FROM sessions.sessions WHERE id = $1")
            .bind(f.session_id)
            .fetch_one(f.handle.pool())
            .await?;
    let base = serve(&f).await?;
    let all: Value = reqwest::get(format!("{base}/api/sessions/{uuid}/verdicts"))
        .await?
        .json()
        .await?;
    assert_eq!(all["total"], 2, "{all}");
    let ids: Vec<i64> = all["verdicts"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v["id"].as_i64())
        .collect();
    assert!(ids.contains(&live) && ids.contains(&eval));
    assert_eq!(
        all["verdicts"][0]["input_text"].as_str(),
        Some(input.as_str())
    );
    let live_only: Value = reqwest::get(format!("{base}/api/sessions/{uuid}/verdicts?mode=live"))
        .await?
        .json()
        .await?;
    assert_eq!(live_only["total"], 1);
    assert_eq!(live_only["end"]["label"], "closed_in_frustration");
    let bad = reqwest::get(format!("{base}/api/sessions/{uuid}/verdicts?mode=draft")).await?;
    assert_eq!(bad.status().as_u16(), 400);
    Ok(())
}

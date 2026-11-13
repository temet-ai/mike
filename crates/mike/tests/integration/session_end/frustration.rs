use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::persistence::eval_repo::EvalPersist as _;
use mike::store::persistence::operator_turns::correction::COUNTS_AS_CORRECTION_SQL;
use mike::store::persistence::operator_turns::projection::{ProjectionMode, ProjectionParams};
use mike::store::persistence::turn_verdicts::VerdictMode;
use serde_json::Value;
use tokio::net::TcpListener;

use crate::judge_quality::promotion_support::{bundle, closed_turn, facts, projected, verdict};
use crate::turn_facts::{Fixture, fixture};

pub(crate) async fn serve(f: &Fixture) -> anyhow::Result<String> {
    let args = vec![
        "mike".to_string(),
        "--inspect-listen-addr".to_string(),
        "127.0.0.1:0".to_string(),
        "--persist-enabled".to_string(),
        "1".to_string(),
        "--database-url".to_string(),
        f.db.database_url.clone(),
    ];
    let state = AppState::new(ProxyConfig::try_parse_from(args)?).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    let router = mike::app::inspect_router_on(state, listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Ok(base)
}

async fn gold_end(f: &Fixture, turn: i64, label: &str) -> anyhow::Result<i64> {
    crate::turn_facts::dev_split(f, turn).await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid,
                                  axis, label, tier, method, split, source)
         SELECT 'session_end', t.id, t.id, t.session_id, s.session_uuid, 'session_end', $2, 'gold',
                'operator_click', 'dev', 'test'
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = $1
         RETURNING id",
    )
    .bind(turn)
    .bind(label)
    .fetch_one(f.handle.pool())
    .await?)
}

async fn counts_as_correction(f: &Fixture, turn: i64) -> anyhow::Result<bool> {
    let sql = format!(
        "SELECT {COUNTS_AS_CORRECTION_SQL}
           FROM memory.operator_turns t JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
          WHERE t.id = $1"
    );
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(turn)
        .fetch_one(f.handle.pool())
        .await?)
}

fn params(turn_bundle: i64, session_bundle: i64) -> ProjectionParams {
    ProjectionParams {
        live_bundle_id: turn_bundle,
        confidence_threshold: 0.5,
        mode: ProjectionMode::Apply,
        session_judge_live_bundle_id: session_bundle,
    }
}

#[tokio::test]
async fn frustration_close_projects_the_final_turn_as_a_correction() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let ending = closed_turn(&f, 30_001).await?;
    let resolved = closed_turn(&f, 30_002).await?;
    facts(&f, ending, "genuine", "none_session_settled").await?;
    facts(&f, resolved, "genuine", "resolved").await?;
    for turn in [ending, resolved] {
        let mut judgement = verdict(turn, VerdictMode::Live, b, ("accepted", "delivered"), 0.9);
        judgement.next_genuine_request_id = (turn == resolved).then_some(2);
        f.handle.eval().insert_turn_verdict(judgement).await?;
        gold_end(&f, turn, "closed_in_frustration").await?;
    }
    f.handle
        .project_turn_outcomes(&[ending, resolved], &params(b, 0))
        .await?;
    let p = projected(&f, ending).await?;
    assert_eq!(p.outcome, "abandoned");
    assert_eq!(p.outcome_source.as_deref(), Some("frustration_close"));
    assert_eq!(
        p.reaction.as_deref(),
        Some("accepted"),
        "the chips keep the turn verdict"
    );
    assert!(counts_as_correction(&f, ending).await?);
    let r = projected(&f, resolved).await?;
    assert_eq!(
        r.outcome, "accepted",
        "a turn that got a reply is not an ending turn"
    );
    assert!(!counts_as_correction(&f, resolved).await?);
    Ok(())
}

#[tokio::test]
async fn gold_reaction_outranks_the_session_end_label() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let ending = closed_turn(&f, 30_101).await?;
    facts(&f, ending, "genuine", "none_session_settled").await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            ending,
            VerdictMode::Live,
            b,
            ("accepted", "delivered"),
            0.9,
        ))
        .await?;
    gold_end(&f, ending, "closed_in_frustration").await?;
    crate::judge_quality::promotion_support::gold(&f, ending, "reaction", "accepted").await?;
    f.handle
        .project_turn_outcomes(&[ending], &params(b, 0))
        .await?;
    let p = projected(&f, ending).await?;
    assert_ne!(p.outcome, "abandoned");
    assert_ne!(p.outcome_source.as_deref(), Some("frustration_close"));
    Ok(())
}

#[tokio::test]
async fn list_corrected_turns_and_summary_count_a_frustration_close() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let ending = closed_turn(&f, 30_201).await?;
    facts(&f, ending, "genuine", "none_session_settled").await?;
    f.handle
        .eval()
        .insert_turn_verdict(verdict(
            ending,
            VerdictMode::Live,
            b,
            ("accepted", "delivered"),
            0.9,
        ))
        .await?;
    gold_end(&f, ending, "closed_in_frustration").await?;
    f.handle
        .project_turn_outcomes(&[ending], &params(b, 0))
        .await?;
    let base = serve(&f).await?;
    let listed: Value = reqwest::get(format!(
        "{base}/api/operator-turns/by-outcome?outcome=correction&since=2000-01-01T00:00:00Z"
    ))
    .await?
    .json()
    .await?;
    let rows = listed.as_array().cloned().unwrap_or_default();
    assert!(
        rows.iter()
            .any(|r| r["turn_id"] == ending && r["counts_as_correction"] == true),
        "{listed}"
    );
    let summary: Value = reqwest::get(format!("{base}/api/operator-turns/outcome-summary"))
        .await?
        .json()
        .await?;
    assert_eq!(summary["corrections"], 1, "{summary}");
    assert_eq!(summary["frustration_closes"], 1, "{summary}");
    Ok(())
}

#[test]
fn projection_writes_abandoned_only_in_the_frustration_branch() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/store/persistence/operator_turns/projection.sql");
    let sql = std::fs::read_to_string(path)?;
    let abandoned: Vec<&str> = sql.lines().filter(|l| l.contains("'abandoned'")).collect();
    assert_eq!(abandoned.len(), 1, "{abandoned:?}");
    assert!(sql.contains("frustration_close"));
    Ok(())
}

#[tokio::test]
async fn an_abandoned_turn_without_an_outcome_source_is_not_a_correction_and_still_loads()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = closed_turn(&f, 30_301).await?;
    sqlx::query(
        "UPDATE memory.operator_turns
            SET outcome_id = (SELECT id FROM memory.turn_outcomes WHERE code = 'abandoned'),
                outcome_source = NULL
          WHERE id = $1",
    )
    .bind(turn)
    .execute(f.handle.pool())
    .await?;
    assert!(!counts_as_correction(&f, turn).await?);
    let base = serve(&f).await?;
    let response = reqwest::get(format!("{base}/api/operator-turns/{turn}")).await?;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await?;
    assert_eq!(body["counts_as_correction"], false, "{body}");
    Ok(())
}

#[tokio::test]
async fn resumed_session_clears_frustration_projection_without_a_turn_verdict() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let b = bundle(&f).await?;
    let turn = closed_turn(&f, 30_901).await?;
    facts(&f, turn, "genuine", "none_session_settled").await?;
    gold_end(&f, turn, "closed_in_frustration").await?;
    let p = params(b, 0);
    f.handle.project_turn_outcomes(&[turn], &p).await?;
    let before = projected(&f, turn).await?;
    assert_eq!(before.outcome_source.as_deref(), Some("frustration_close"));
    assert_eq!(before.live_verdict_id, None);
    facts(&f, turn, "genuine", "resolved").await?;
    f.handle.reproject_final_turns(&p, 7).await?;
    let after = projected(&f, turn).await?;
    assert_eq!(after.outcome_source.as_deref(), Some("no_verdict"));
    assert_eq!(after.outcome, "unknown");
    assert!(f.handle.reproject_final_turns(&p, 7).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

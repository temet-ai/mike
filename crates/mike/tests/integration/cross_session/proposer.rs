use mike::store::persistence::cross_session::ProposerParams;
use mike::store::persistence::turn_verdicts::VerdictMode;

use crate::judge_quality::promotion_support::{bundle, verdict};
use crate::turn_facts::{
    Fixture, NewTurn, Req, fixture, insert_request_in, insert_session, insert_turn_in, sample_row,
};

fn params() -> ProposerParams<'static> {
    ProposerParams {
        window_days: 30,
        min_sessions: 2,
        min_confirmed: 3,
        rollup_id: 1,
        version: "proposer-v1",
        reject_cooldown_days: 30,
        max_new_per_run: 3,
        max_open: 10,
        operator_min_scopes: 3,
    }
}

async fn directive(
    f: &Fixture,
    session: i64,
    request: i64,
    correcting: bool,
) -> anyhow::Result<()> {
    if correcting {
        let prev_request = request - 1;
        insert_request_in(f, session, &Req::primary(prev_request, 0, "add the retry")).await?;
        let prev = insert_turn_in(
            f,
            session,
            &NewTurn::closed(prev_request, prev_request, 0, 30),
        )
        .await?;
        let mut row = sample_row(prev, session, chrono::Utc::now());
        row.next_state = "resolved".to_owned();
        row.next_genuine_request_id = Some(request);
        f.handle.upsert_turn_facts(&[row]).await?;
        let b = bundle(f).await?;
        f.handle
            .insert_turn_verdict(verdict(
                prev,
                VerdictMode::Live,
                b,
                ("corrected", "partial"),
                0.9,
            ))
            .await?;
    }
    insert_request_in(
        f,
        session,
        &Req::primary(request, 40, "just commit when the tests are green"),
    )
    .await?;
    let turn = insert_turn_in(f, session, &NewTurn::closed(request, request, 40, 60)).await?;
    sqlx::query("UPDATE memory.operator_turns SET user_prompt = $2 WHERE id = $1")
        .bind(turn)
        .bind("just commit when the tests are green")
        .execute(f.handle.pool())
        .await?;
    f.handle
        .upsert_turn_facts(&[sample_row(turn, session, chrono::Utc::now())])
        .await?;
    sqlx::query(
        "INSERT INTO memory.dossier_entries (session_id, operator_turn_id, kind, data, source, source_ref, dedup_key,
                                             extractor_version, effective_at)
         VALUES ($1, $2, 'directive', '{\"class\": \"commit\", \"polarity\": \"allow\", \"span\": [0, 11]}'::jsonb,
                 'capture', 'requests.requests:' || $3::text, 'request:' || $3::text || ':commit:allow',
                 'directives-v1', now() - interval '1 day')",
    )
    .bind(session)
    .bind(turn)
    .bind(request)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn restated_directives_in_two_sessions_produce_one_proposal_with_evidence_and_reject_records_it()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let second = insert_session(f.handle.pool(), f.project_id, chrono::Utc::now()).await?;
    directive(&f, f.session_id, 90_010, true).await?;
    directive(&f, second, 90_110, false).await?;
    assert_eq!(f.handle.propose_repeated_directives(&params()).await?, 1);
    let proposals = f
        .handle
        .list_directive_proposals(Some("open"), None, 50)
        .await?;
    assert_eq!(proposals.len(), 1);
    assert_eq!(
        (proposals[0].class.as_str(), proposals[0].polarity.as_str()),
        ("commit", "allow")
    );
    assert_eq!(proposals[0].session_count, 2);
    assert_eq!(
        proposals[0].proposed_text,
        "just commit when the tests are green"
    );
    let detail = f.handle.fetch_proposal_detail(proposals[0].id).await?;
    assert_eq!(detail.map(|d| d.evidence.len()), Some(2));
    assert_eq!(
        f.handle.propose_repeated_directives(&params()).await?,
        0,
        "an open key is not proposed twice"
    );
    assert!(
        f.handle
            .reject_directive_proposal(proposals[0].id, Some("not a standing rule"))
            .await?
    );
    assert!(
        !f.handle
            .reject_directive_proposal(proposals[0].id, None)
            .await?,
        "only an open proposal can be rejected"
    );
    let status: String =
        sqlx::query_scalar("SELECT status FROM memory.directive_proposals WHERE id = $1")
            .bind(proposals[0].id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(status, "rejected");
    Ok(())
}

#[tokio::test]
async fn a_directive_in_one_session_produces_no_proposal() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    directive(&f, f.session_id, 91_010, true).await?;
    assert_eq!(f.handle.propose_repeated_directives(&params()).await?, 0);
    Ok(())
}

use super::{
    Fixture, NewTurn, Req, attach, fixture, insert_request, insert_response, insert_text_block,
    insert_turn,
};
use mike::store::persistence::dossier_repo::DossierPersist as _;

async fn text_request(
    f: &Fixture,
    id: i64,
    origin: &str,
    at_secs: i64,
    stop: Option<&str>,
) -> anyhow::Result<()> {
    insert_request(f, &Req::lane(id, origin, at_secs)).await?;
    insert_text_block(f, id, 0, "text").await?;
    if let Some(reason) = stop {
        insert_response(f, id, reason).await?;
    }
    Ok(())
}

#[tokio::test]
async fn stop_facts_take_last_primary_text_request_and_its_stop_reason() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    text_request(&f, 9101, "primary", 5, Some("max_tokens")).await?;
    text_request(&f, 9102, "monitor_sidecar", 6, Some("end_turn")).await?;
    let turn = insert_turn(
        &f,
        &NewTurn {
            evidence_request_id: Some(9100),
            ..NewTurn::closed(9100, 9102, 0, 50)
        },
    )
    .await?;
    attach(&f, turn, &[9100, 9101, 9102]).await?;
    let facts = f.handle.dossier().load_stop_facts(&[turn]).await?;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].stop_request_id, Some(9101));
    assert_eq!(facts[0].stop_reason.as_deref(), Some("max_tokens"));
    assert_eq!(facts[0].close_cause, "next_prompt");
    assert!(!facts[0].interrupted);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn stop_request_without_response_is_aborted() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    text_request(&f, 9101, "primary", 5, None).await?;
    let turn = insert_turn(&f, &NewTurn::closed(9100, 9101, 0, 50)).await?;
    attach(&f, turn, &[9100, 9101]).await?;
    let facts = f.handle.dossier().load_stop_facts(&[turn]).await?;
    assert_eq!(facts[0].stop_reason.as_deref(), Some("aborted"));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn turn_with_no_text_request_has_no_stop_request() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    let turn = insert_turn(&f, &NewTurn::closed(9100, 9100, 0, 50)).await?;
    attach(&f, turn, &[9100]).await?;
    let facts = f.handle.dossier().load_stop_facts(&[turn]).await?;
    assert_eq!(facts[0].stop_request_id, None);
    assert_eq!(facts[0].stop_reason, None);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn interrupted_marker_on_a_primary_request_sets_interrupted() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    insert_request(&f, &Req::primary(9101, 5, "[Request interrupted by user]")).await?;
    insert_request(&f, &Req::primary(9102, 9, "next")).await?;
    let turn = insert_turn(
        &f,
        &NewTurn {
            evidence_request_id: Some(9102),
            ..NewTurn::closed(9100, 9101, 0, 9)
        },
    )
    .await?;
    attach(&f, turn, &[9100, 9101]).await?;
    let facts = f.handle.dossier().load_stop_facts(&[turn]).await?;
    assert!(facts[0].interrupted);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn close_cause_is_abort_when_stream_aborted_error_matches_closed_at() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    let aborted = Req {
        completed_secs: Some(50),
        ..Req::lane(9101, "primary", 5)
    };
    insert_request(&f, &aborted).await?;
    sqlx::query(
        "INSERT INTO requests.errors (request_id, message) VALUES (9101, 'stream aborted')",
    )
    .execute(f.handle.pool())
    .await?;
    let turn = insert_turn(&f, &NewTurn::closed(9100, 9101, 0, 50)).await?;
    attach(&f, turn, &[9100, 9101]).await?;
    let facts = f.handle.dossier().load_stop_facts(&[turn]).await?;
    assert_eq!(facts[0].close_cause, "abort");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn close_cause_is_idle_reap_without_evidence_or_abort() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    insert_request(&f, &Req::primary(9100, 0, "ask")).await?;
    let turn = insert_turn(&f, &NewTurn::closed(9100, 9100, 0, 50)).await?;
    attach(&f, turn, &[9100]).await?;
    let facts = f.handle.dossier().load_stop_facts(&[turn]).await?;
    assert_eq!(facts[0].close_cause, "idle_reap");
    f.db.cleanup().await?;
    Ok(())
}

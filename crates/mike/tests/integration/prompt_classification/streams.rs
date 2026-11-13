use chrono::Duration;
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::prompt_stream::{StreamSource, StreamWindow};

use super::{Fixture, NewRequest, fixture, insert_request, insert_turn, plain_request, write_of};

fn window(f: &Fixture, from: i64, to: i64) -> StreamWindow {
    StreamWindow {
        from: f.base + Duration::seconds(from),
        to: f.base + Duration::seconds(to),
    }
}

#[tokio::test]
async fn load_prompt_streams_stored_excludes_empty_prompt_and_orders_by_request_id()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9005, 50, "later").await?;
    plain_request(&f, 9003, 30, "").await?;
    plain_request(&f, 9004, 40, "middle").await?;
    for (id, kind) in [
        (9005, "operator"),
        (9003, "empty-prompt"),
        (9004, "loop-wakeup"),
    ] {
        f.handle
            .write_classification_row(&write_of(&f, id, kind))
            .await?;
    }
    let turn_at = f.base + Duration::seconds(50);
    let turn_id = insert_turn(&f, f.session_id, 9005, Some("operator"), turn_at).await?;
    let rows = f
        .handle
        .dossier()
        .load_prompt_streams(&[f.session_id], window(&f, 0, 100), StreamSource::Stored)
        .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.request_id).collect();
    assert_eq!(ids, vec![9004, 9005]);
    assert_eq!(rows[0].kind_code.as_deref(), Some("loop-wakeup"));
    assert_eq!(rows[0].turn_id, None);
    assert_eq!(rows[1].turn_id, Some(turn_id));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn load_prompt_streams_j2_text_includes_orphan_requests_with_text() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    plain_request(&f, 9001, 10, "turn opener").await?;
    plain_request(&f, 9002, 20, "orphan with text").await?;
    plain_request(&f, 9003, 30, "").await?;
    insert_request(
        f.handle.pool(),
        &NewRequest {
            id: 9004,
            session_id: f.session_id,
            project_id: f.project_id,
            at: f.base + Duration::seconds(40),
            human_prompt: "agent prompt",
            author: "agent",
            agent_id: Some("agent-1"),
        },
    )
    .await?;
    let turn_at = f.base + Duration::seconds(10);
    let turn_id = insert_turn(&f, f.session_id, 9001, None, turn_at).await?;
    sqlx::query("UPDATE requests.requests SET operator_turn_id = $2 WHERE id = $1")
        .bind(9001_i64)
        .bind(turn_id)
        .execute(f.handle.pool())
        .await?;
    let rows = f
        .handle
        .dossier()
        .load_prompt_streams(&[f.session_id], window(&f, 0, 100), StreamSource::J2Text)
        .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.request_id).collect();
    assert_eq!(ids, vec![9001, 9002, 9004]);
    assert_eq!(rows[0].turn_id, Some(turn_id));
    assert_eq!(rows[0].text.as_deref(), Some("prompt text"));
    assert_eq!(rows[1].text.as_deref(), Some("orphan with text"));
    assert!(rows[1].turn_id.is_none());
    assert_eq!(rows[2].author.as_deref(), Some("agent"));
    assert!(rows[2].has_agent_id);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn load_prompt_streams_respects_created_at_window() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    for (id, offset) in [(9001, 0), (9002, 60), (9003, 120)] {
        plain_request(&f, id, offset, "text").await?;
        f.handle
            .write_classification_row(&write_of(&f, id, "operator"))
            .await?;
    }
    let rows = f
        .handle
        .dossier()
        .load_prompt_streams(&[f.session_id], window(&f, 30, 90), StreamSource::Stored)
        .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.request_id).collect();
    assert_eq!(ids, vec![9002]);
    let empty = f
        .handle
        .dossier()
        .load_prompt_streams(&[], window(&f, 0, 200), StreamSource::Stored)
        .await?;
    assert!(empty.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

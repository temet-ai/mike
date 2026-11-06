use super::{NewTurn, Req, attach, fixture, insert_request, insert_text_block, insert_turn};

async fn seed_closed_turn(f: &super::Fixture, final_text: Option<&str>) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(9100, 0, "ask")).await?;
    insert_request(f, &Req::lane(9101, "subagent", 11)).await?;
    insert_request(f, &Req::lane(9102, "monitor_sidecar", 12)).await?;
    insert_text_block(f, 9100, 0, "main ").await?;
    insert_text_block(f, 9101, 0, "agent").await?;
    insert_text_block(f, 9102, 0, "noise").await?;
    let turn = insert_turn(
        f,
        &NewTurn {
            final_text,
            ..NewTurn::closed(9100, 9102, 0, 100)
        },
    )
    .await?;
    attach(f, turn, &[9100, 9101, 9102]).await?;
    Ok(turn)
}

async fn is_consistent(f: &super::Fixture, turn: i64) -> anyhow::Result<bool> {
    let facts = f.handle.load_text_facts(&[turn]).await?;
    Ok(facts.first().is_some_and(|t| t.consistent))
}

#[tokio::test]
async fn refresh_final_assistant_text_missing_snapshot_rebuilt_once() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_closed_turn(&f, None).await?;
    assert!(!is_consistent(&f, turn).await?);
    assert_eq!(f.handle.refresh_final_assistant_text(&[turn]).await?, 1);
    assert!(is_consistent(&f, turn).await?);
    assert_eq!(f.handle.refresh_final_assistant_text(&[turn]).await?, 0);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_final_assistant_text_partial_snapshot_rebuilt() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_closed_turn(&f, Some("agent")).await?;
    assert!(!is_consistent(&f, turn).await?);
    assert_eq!(f.handle.refresh_final_assistant_text(&[turn]).await?, 1);
    assert!(is_consistent(&f, turn).await?);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_final_assistant_text_no_turns_changes_nothing() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert_eq!(f.handle.refresh_final_assistant_text(&[]).await?, 0);
    f.db.cleanup().await?;
    Ok(())
}

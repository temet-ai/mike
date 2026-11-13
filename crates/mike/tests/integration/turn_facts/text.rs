use anyhow::Context;
use mike::store::persistence::dossier_repo::DossierPersist as _;

use super::{NewTurn, Req, attach, fixture, insert_request, insert_text_block, insert_turn};

async fn seed_turn_with_blocks(
    f: &super::Fixture,
    blocks: &[(i64, &str, i32, String)],
) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(9100, 0, "ask")).await?;
    let mut all = vec![9100];
    let mut final_text = String::new();
    for (id, origin, ord, content) in blocks {
        if !all.contains(id) {
            insert_request(f, &Req::lane(*id, origin, 10 + *id - 9100)).await?;
            all.push(*id);
        }
        insert_text_block(f, *id, *ord, content).await?;
        if !origin.ends_with("sidecar") {
            final_text.push_str(content);
        }
    }
    let last = *all.last().unwrap_or(&9100);
    let turn = insert_turn(
        f,
        &NewTurn {
            final_text: Some(&final_text),
            ..NewTurn::closed(9100, last, 0, 100)
        },
    )
    .await?;
    attach(f, turn, &all).await?;
    Ok(turn)
}

#[tokio::test]
async fn text_cut_at_before_trailing_monitor_and_suggestion_blocks() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_blocks(
        &f,
        &[
            (9101, "primary", 0, "a".repeat(4584)),
            (9102, "monitor_sidecar", 0, "m".repeat(142)),
            (9103, "next_input_suggestion_sidecar", 0, "s".repeat(28)),
        ],
    )
    .await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].text_chars, 4754);
    assert_eq!(facts[0].text_cut_at, 4584);
    assert_eq!(
        facts[0].last_text_origin.as_deref(),
        Some("next_input_suggestion_sidecar")
    );
    assert!(facts[0].consistent);
    let spans = facts[0].text_spans.as_array().context("spans array")?;
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[1]["origin"], "monitor_sidecar");
    assert_eq!(spans[1]["start"], 4584);
    assert_eq!(spans[1]["end"], 4726);
    assert_eq!(spans[2]["first_request_id"], 9103);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn text_spans_group_consecutive_blocks_by_origin() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_blocks(
        &f,
        &[
            (9101, "primary", 0, "aaa".to_string()),
            (9101, "primary", 1, "bb".to_string()),
            (9102, "primary", 0, "c".to_string()),
            (9103, "monitor_sidecar", 0, "dddd".to_string()),
        ],
    )
    .await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    let spans = facts[0].text_spans.as_array().context("spans array")?;
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0]["origin"], "primary");
    assert_eq!(spans[0]["start"], 0);
    assert_eq!(spans[0]["end"], 6);
    assert_eq!(spans[0]["first_request_id"], 9101);
    assert_eq!(spans[0]["last_request_id"], 9102);
    assert_eq!(facts[0].text_cut_at, 6);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn turn_with_only_sidecar_text_has_cut_at_zero() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn =
        seed_turn_with_blocks(&f, &[(9101, "monitor_sidecar", 0, "status".to_string())]).await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    assert_eq!(facts[0].text_chars, 6);
    assert_eq!(facts[0].text_cut_at, 0);
    assert!(facts[0].consistent);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn turn_without_text_has_zero_chars() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_blocks(&f, &[]).await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].text_chars, 0);
    assert_eq!(facts[0].text_cut_at, 0);
    assert_eq!(facts[0].last_text_origin, None);
    assert_eq!(facts[0].text_spans, serde_json::json!([]));
    assert!(facts[0].consistent);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn mid_turn_advisor_block_is_a_span_not_behind_cut() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_blocks(
        &f,
        &[
            (9101, "primary", 0, "first ".to_string()),
            (9102, "advisor_sidecar", 0, "advice ".to_string()),
            (9103, "primary", 0, "last".to_string()),
        ],
    )
    .await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    assert_eq!(facts[0].text_chars, 17);
    assert_eq!(facts[0].text_cut_at, 17);
    assert!(facts[0].consistent);
    let spans = facts[0].text_spans.as_array().context("spans array")?;
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[1]["origin"], "advisor_sidecar");
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn multibyte_text_offsets_count_characters() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_blocks(
        &f,
        &[
            (9101, "primary", 0, "Zażółć gęślą".to_string()),
            (9102, "monitor_sidecar", 0, "jaźń".to_string()),
        ],
    )
    .await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    assert_eq!(facts[0].text_cut_at, 12);
    assert_eq!(facts[0].text_chars, 16);
    assert!(facts[0].consistent);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn text_mismatch_with_final_assistant_text_marks_row_stale() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_blocks(&f, &[(9101, "primary", 0, "abc".to_string())]).await?;
    insert_text_block(&f, 9101, 1, "later").await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    assert_eq!(facts[0].text_chars, 8);
    assert!(!facts[0].consistent);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn load_text_facts_with_no_turns_is_empty() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    assert!(f.handle.dossier().load_text_facts(&[]).await?.is_empty());
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn sidecar_contamination_in_final_text_is_still_inconsistent() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let turn = seed_turn_with_blocks(
        &f,
        &[
            (9101, "primary", 0, "answer".to_string()),
            (9102, "monitor_sidecar", 0, "status".to_string()),
        ],
    )
    .await?;
    sqlx::query(
        "UPDATE memory.operator_turns SET final_assistant_text = 'answerstatus' WHERE id = $1",
    )
    .bind(turn)
    .execute(f.handle.pool())
    .await?;
    let facts = f.handle.dossier().load_text_facts(&[turn]).await?;
    assert_eq!(facts[0].text_chars, 12);
    assert!(!facts[0].consistent);
    assert_eq!(
        facts[0].text_spans.as_array().context("spans array")?.len(),
        2
    );
    f.db.cleanup().await?;
    Ok(())
}

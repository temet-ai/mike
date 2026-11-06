use mike::semantic::SemanticError;
use mike::store::persistence::semantic::NewItem;

use super::support::{build_all, closed_turn, embedder, embedding_stub, facts, service};
use crate::turn_facts::fixture;

#[tokio::test]
async fn egress_blocked_status_when_url_is_remote() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let svc = service("http://embeddings.example.invalid/v1");
    let result = mike::semantic::check::ensure_embedder(&f.handle, &svc, false).await;
    assert!(
        matches!(result, Err(SemanticError::EgressBlocked)),
        "{result:?}"
    );
    assert!(f.handle.current_semantic_embedder().await?.is_none());
    Ok(())
}

#[tokio::test]
async fn loopback_url_embeds_and_the_backfill_is_idempotent() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let stub = embedding_stub().await;
    let svc = service(&stub.uri());
    for (i, prompt) in ["fix the parser", "add the retry", "ship the release"]
        .iter()
        .enumerate()
    {
        let id = 30_000 + i64::try_from(i)?;
        let turn = closed_turn(&f, id, prompt, i64::try_from(i)? * 100).await?;
        facts(&f, turn, None).await?;
    }
    let e = embedder(&f, &svc).await?;
    assert!(
        e.embedder_key
            .starts_with("nomic-embed-text-v1.5|768|1024|doc-prefix-v1|since:")
    );
    assert_eq!(build_all(&f, &svc, &e).await?, 3);
    let items: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.semantic_items WHERE embedder_key = $1")
            .bind(&e.embedder_key)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(items, 6, "one prompt and one ending item per turn");
    assert_eq!(
        build_all(&f, &svc, &e).await?,
        0,
        "a second run finds nothing to build"
    );
    let again = embedder(&f, &svc).await?;
    assert_eq!(again.id, e.id, "the same canaries keep the embedder");
    Ok(())
}

#[tokio::test]
async fn semantic_anchor_without_origin_is_rejected_by_check() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let stub = embedding_stub().await;
    let e = embedder(&f, &service(&stub.uri())).await?;
    let anchor = NewItem {
        kind: "anchor",
        source_ref: "mirror:empty-turn:1".to_owned(),
        pattern: Some("empty-turn".to_owned()),
        text: "you did nothing".to_owned(),
        embedding: super::support::vector_for("you did nothing"),
        embedder_key: e.embedder_key.clone(),
        vector_source: "embedded",
        builder_version: "semantic-items-v1",
        effective_at: chrono::Utc::now(),
        data: serde_json::json!({}),
        ..NewItem::default()
    };
    assert!(f.handle.insert_semantic_items(&[anchor]).await.is_err());
    Ok(())
}

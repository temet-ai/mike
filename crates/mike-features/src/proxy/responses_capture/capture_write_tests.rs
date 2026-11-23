use super::*;
use crate::store::persistence::test_support;

#[tokio::test]
async fn changed_or_inserted_prefix_keeps_reusing_the_shared_suffix() -> anyhow::Result<()> {
    let db = test_support::acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture matching requires an isolated test database"))?;
    let persist = PersistHandle::connect(&db.database_url).await?;
    let previous = persist.runtime().create_capture_document(None).await?;
    let original: Vec<_> = (0..32).map(|index| vec![index as u8; 4096]).collect();
    let input: Vec<_> = original
        .iter()
        .cloned()
        .map(CaptureChunkInput::New)
        .collect();
    let old_ids = persist
        .runtime()
        .append_capture_chunks(previous, 0, &input)
        .await?;
    persist
        .runtime()
        .finish_capture_document(previous, true)
        .await?;
    for insert in [false, true] {
        let document = persist
            .runtime()
            .create_capture_document(Some(previous))
            .await?;
        let mut writer = Writer {
            persist: &persist,
            document,
            predecessor: Some(previous),
            candidates: VecDeque::new(),
            anchor: Vec::new(),
            next_candidate: 0,
            ordinal: 0,
            pending: Vec::new(),
        };
        for index in 0..12 {
            writer
                .push(vec![100 + index; 4096])
                .await
                .map_err(anyhow::Error::msg)?;
        }
        let suffix = if insert {
            &original[..]
        } else {
            &original[12..]
        };
        for chunk in suffix {
            writer
                .push(chunk.clone())
                .await
                .map_err(anyhow::Error::msg)?;
        }
        writer.flush().await.map_err(anyhow::Error::msg)?;
        persist
            .runtime()
            .finish_capture_document(document, true)
            .await?;
        let saved = persist
            .runtime()
            .capture_document_chunks(document, 0, 100)
            .await?;
        let expected_ids = if insert { &old_ids[..] } else { &old_ids[12..] };
        assert_eq!(
            saved[12..].iter().map(|chunk| chunk.id).collect::<Vec<_>>(),
            expected_ids
        );
    }
    persist.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

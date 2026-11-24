use serde_json::{Value as JsonValue, json};

use super::{
    Capture, SentColumns, capture, clear_request_body, original_of, sent_columns, sent_of,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support;

fn conversation(turns: usize) -> JsonValue {
    let messages: Vec<JsonValue> = (0..turns)
        .map(|n| {
            let role = if n % 2 == 0 { "user" } else { "assistant" };
            json!({"role": role, "content": [{"type": "text", "text": format!("turn {n}")}]})
        })
        .collect();
    json!({
        "model": "m",
        "system": [{"type": "text", "text": "S"}],
        "messages": messages,
        "tools": [{"name": "Read", "input_schema": {"type": "object"}}],
        "stream": true
    })
}

fn with(mut body: JsonValue, edit: impl FnOnce(&mut JsonValue)) -> JsonValue {
    edit(&mut body);
    body
}

async fn assert_served_without_body(
    handle: &PersistHandle,
    request_id: i64,
    original: &JsonValue,
    sent: &JsonValue,
) -> anyhow::Result<()> {
    assert_eq!(
        &sent_of(handle, request_id).await?,
        sent,
        "sent, body still stored"
    );
    clear_request_body(handle, request_id).await?;
    assert_eq!(
        &sent_of(handle, request_id).await?,
        sent,
        "sent, body cleared"
    );
    assert_eq!(
        &original_of(handle, request_id).await?,
        original,
        "original, body cleared"
    );
    Ok(())
}

async fn capture_one(
    handle: &PersistHandle,
    original: &JsonValue,
    sent: Option<&JsonValue>,
) -> anyhow::Result<i64> {
    let session_id = uuid::Uuid::new_v4().to_string();
    capture(
        handle,
        Capture {
            session_id: &session_id,
            original,
            sent,
        },
    )
    .await
}

#[tokio::test]
async fn capture_without_edits_stores_no_sent_content() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let original = conversation(3);
    for sent in [None, Some(&original)] {
        let id = capture_one(&handle, &original, sent).await?;
        let none = SentColumns {
            has_envelope: false,
            count: None,
            ords: vec![],
        };
        assert_eq!(sent_columns(&handle, id).await?, none);
        assert_served_without_body(&handle, id, &original, &original).await?;
    }
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capture_system_prompt_edit_stores_only_the_envelope() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let original = conversation(3);
    let sent = with(original.clone(), |b| {
        b["system"][0]["text"] = json!("S and the composer")
    });
    let id = capture_one(&handle, &original, Some(&sent)).await?;
    let columns = SentColumns {
        has_envelope: true,
        count: None,
        ords: vec![],
    };
    assert_eq!(sent_columns(&handle, id).await?, columns);
    let (envelope,): (Vec<u8>,) =
        sqlx::query_as("SELECT sent_envelope FROM requests.requests WHERE id = $1")
            .bind(id)
            .fetch_one(&handle.pool)
            .await?;
    let envelope = String::from_utf8(envelope)?;
    assert!(
        envelope.contains(r#""tools":[]"#),
        "unchanged tools are left out: {envelope}"
    );
    assert!(
        envelope.contains(r#""messages":[]"#),
        "messages are left out: {envelope}"
    );
    assert_served_without_body(&handle, id, &original, &sent).await?;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capture_added_message_edit_stores_that_ord() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let original = conversation(3);
    let sent = with(original.clone(), |b| {
        b["messages"][2]["content"][0]["text"] = json!("turn 2 <proxy-lessons>x</proxy-lessons>");
    });
    let id = capture_one(&handle, &original, Some(&sent)).await?;
    let columns = SentColumns {
        has_envelope: false,
        count: None,
        ords: vec![2],
    };
    assert_eq!(sent_columns(&handle, id).await?, columns);
    assert_served_without_body(&handle, id, &original, &sent).await?;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capture_carried_message_edit_stores_it_and_reuses_the_predecessors_row()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let suppress =
        |b: &mut JsonValue| b["messages"][0]["content"][0]["text"] = json!("[read removed]");
    let mut ids = Vec::new();
    for turns in [1, 3, 5] {
        let original = conversation(turns);
        let sent = with(original.clone(), suppress);
        let id = capture(
            &handle,
            Capture {
                session_id: &session_id,
                original: &original,
                sent: Some(&sent),
            },
        )
        .await?;
        assert_eq!(sent_columns(&handle, id).await?.ords, vec![0]);
        assert_served_without_body(&handle, id, &original, &sent).await?;
        ids.push(id);
    }
    let message_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT message_id FROM requests.sent_messages WHERE request_id = ANY($1) ORDER BY request_id",
    )
    .bind(&ids)
    .fetch_all(&handle.pool)
    .await?;
    assert_eq!(message_ids.len(), 3);
    assert!(
        message_ids.windows(2).all(|w| w[0] == w[1]),
        "the same edit to a carried message reuses one messages row: {message_ids:?}"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capture_message_count_change_stores_every_sent_message() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let original = conversation(3);
    let sent = with(original.clone(), |b| {
        if let Some(messages) = b["messages"].as_array_mut() {
            messages.insert(1, json!({"role": "user", "content": "mail from a peer"}));
        }
    });
    let id = capture_one(&handle, &original, Some(&sent)).await?;
    let columns = SentColumns {
        has_envelope: false,
        count: Some(4),
        ords: vec![0, 1, 2, 3],
    };
    assert_eq!(sent_columns(&handle, id).await?, columns);
    assert_served_without_body(&handle, id, &original, &sent).await?;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn capture_tools_edit_keeps_the_sent_tools_inline() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let original = conversation(3);
    let appended = with(original.clone(), |b| {
        if let Some(tools) = b["tools"].as_array_mut() {
            tools.push(json!({"name": "mike_session_roster"}));
        }
    });
    let emptied = with(original.clone(), |b| b["tools"] = json!([]));
    for sent in [appended, emptied] {
        let id = capture_one(&handle, &original, Some(&sent)).await?;
        let columns = SentColumns {
            has_envelope: true,
            count: None,
            ords: vec![],
        };
        assert_eq!(sent_columns(&handle, id).await?, columns);
        assert_served_without_body(&handle, id, &original, &sent).await?;
    }
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_new_capture_stores_no_raw_body_and_serves_both_versions() -> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let original = conversation(3);
    let sent = with(original.clone(), |b| {
        b["messages"][2]["content"][0]["text"] = json!("edited")
    });
    let id = capture_one(&handle, &original, Some(&sent)).await?;
    let stored: (bool, bool, i32) = sqlx::query_as(
        "SELECT request_body IS NULL, request_envelope IS NOT NULL, request_body_size
           FROM requests.requests WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&handle.pool)
    .await?;
    assert_eq!(
        stored,
        (true, true, i32::try_from(original.to_string().len())?),
        "a body with an envelope keeps only its envelope and size"
    );
    assert_eq!(original_of(&handle, id).await?, original);
    assert_eq!(sent_of(&handle, id).await?, sent);
    db.cleanup().await?;
    Ok(())
}

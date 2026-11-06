use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone)]
pub(crate) struct TagRow {
    pub(crate) code: String,
    pub(crate) hue: Option<i16>,
    pub(crate) usage_count: i64,
}

impl PersistHandle {
    pub(crate) async fn fetch_tags(&self) -> Result<Vec<TagRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_tags", async move {
            let rows = sqlx::query_as!(
                TagRow,
                r#"SELECT t.code, t.hue, COUNT(tt.thread_id) AS "usage_count!"
                     FROM mailbox.tags t
                     LEFT JOIN mailbox.thread_tags tt ON tt.tag_id = t.id
                    GROUP BY t.id, t.code, t.hue
                    ORDER BY t.code"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn attach_tags_to_thread(
        &self,
        thread_id: i64,
        codes: &[String],
    ) -> Result<(), PersistError> {
        let trimmed: Vec<String> = codes
            .iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if trimmed.is_empty() {
            return Ok(());
        }
        crate::metrics::time_db("mailbox_attach_tags_to_thread", async move {
            sqlx::query!(
                r"WITH input_codes AS (
                       SELECT DISTINCT n.code
                         FROM UNNEST($1::text[]) AS raw(code)
                         CROSS JOIN LATERAL (SELECT mailbox.normalize_tag(raw.code) AS code) n
                        WHERE n.code IS NOT NULL
                   ),
                   interned AS (
                       INSERT INTO mailbox.tags (code)
                       SELECT code FROM input_codes
                       ON CONFLICT (code) DO SELECT
                       RETURNING id
                   )
                   INSERT INTO mailbox.thread_tags (thread_id, tag_id)
                   SELECT $2, id FROM interned
                   ON CONFLICT DO NOTHING",
                &trimmed,
                thread_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}

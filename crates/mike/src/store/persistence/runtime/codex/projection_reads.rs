use uuid::Uuid;

use crate::runtime::codex::ItemKey;
use crate::runtime::validate_text;
use crate::store::persistence::{PersistError, PersistHandle};

use super::super::invalid;
use super::generation::check_versions;
use super::reads::validate_page;
use super::state::ItemRow;
use super::types::{CodexStoredItem, parse_status};
use super::{
    CodexEffectPage, CodexItemPage, CodexProjectionApplication, CodexProjectionEffect, invariant,
};

struct EffectRow {
    event_id: i64,
    capture_sequence: i64,
    effect_ordinal: i32,
    observation: String,
}

const FIRST_ITEM_SEQUENCE: i64 = 0;
const FIRST_EFFECT_CURSOR: (i64, i32) = (0, -1);

impl TryFrom<EffectRow> for CodexProjectionEffect {
    type Error = PersistError;

    fn try_from(row: EffectRow) -> Result<Self, Self::Error> {
        Ok(Self {
            event_id: row.event_id,
            capture_sequence: row.capture_sequence,
            effect_ordinal: row.effect_ordinal,
            effect: serde_json::from_str(&row.observation)?,
        })
    }
}

fn ensure_known_effect_kind(kind: Option<&str>) -> Result<(), PersistError> {
    if kind
        .is_some_and(|kind| !matches!(kind, "item" | "lifecycle" | "signal" | "usage" | "billing"))
    {
        return Err(invariant("unknown effect kind filter"));
    }
    Ok(())
}

struct ApplicationRow {
    generation_id: Uuid,
    event_id: i64,
    capture_sequence: i64,
    status: String,
    issues: String,
    effect_count: i32,
}

impl TryFrom<ApplicationRow> for CodexProjectionApplication {
    type Error = PersistError;

    fn try_from(row: ApplicationRow) -> Result<Self, Self::Error> {
        Ok(Self {
            generation_id: row.generation_id,
            event_id: row.event_id,
            capture_sequence: row.capture_sequence,
            status: parse_status(&row.status)?,
            issues: serde_json::from_str(&row.issues)?,
            effect_count: row.effect_count,
        })
    }
}

impl PersistHandle {
    async fn validate_codex_generation_read(
        &self,
        generation_id: Uuid,
    ) -> Result<(), PersistError> {
        let generation = self
            .codex_projection_generation(generation_id)
            .await?
            .ok_or_else(|| invariant("generation does not exist"))?;
        check_versions(&generation)
    }

    pub async fn codex_projected_item(
        &self,
        generation_id: Uuid,
        key: &ItemKey,
    ) -> Result<Option<CodexStoredItem>, PersistError> {
        self.validate_codex_generation_read(generation_id).await?;
        let row = sqlx::query_file_as!(
            ItemRow,
            "src/store/persistence/runtime/codex/projected_item.sql",
            generation_id,
            key.namespace_id,
            key.thread_id,
            key.turn_id,
            key.item_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn list_codex_items(
        &self,
        page: CodexItemPage,
    ) -> Result<Vec<CodexStoredItem>, PersistError> {
        validate_page(page.limit)?;
        validate_text(&page.thread_id, "thread_id").map_err(invalid)?;
        self.validate_codex_generation_read(page.generation_id)
            .await?;
        let (after_sequence, after_turn, after_item) =
            page.after
                .unwrap_or((FIRST_ITEM_SEQUENCE, String::new(), String::new()));
        let rows = sqlx::query_file_as!(
            ItemRow,
            "src/store/persistence/runtime/codex/items_page.sql",
            page.generation_id,
            page.thread_id,
            after_sequence,
            after_turn,
            after_item,
            page.limit,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn list_codex_projection_effects(
        &self,
        page: CodexEffectPage,
    ) -> Result<Vec<CodexProjectionEffect>, PersistError> {
        validate_page(page.limit)?;
        validate_text(&page.thread_id, "thread_id").map_err(invalid)?;
        self.validate_codex_generation_read(page.generation_id)
            .await?;
        ensure_known_effect_kind(page.kind.as_deref())?;
        let rows = self.fetch_codex_effect_rows(page).await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn fetch_codex_effect_rows(
        &self,
        page: CodexEffectPage,
    ) -> Result<Vec<EffectRow>, PersistError> {
        let (after_sequence, after_ordinal) = page.after.unwrap_or(FIRST_EFFECT_CURSOR);
        let rows = match page.kind {
            Some(kind) => {
                sqlx::query_file_as!(
                    EffectRow,
                    "src/store/persistence/runtime/codex/effects_page_by_kind.sql",
                    page.generation_id,
                    page.thread_id,
                    kind,
                    after_sequence,
                    after_ordinal,
                    page.limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_file_as!(
                    EffectRow,
                    "src/store/persistence/runtime/codex/effects_page.sql",
                    page.generation_id,
                    page.thread_id,
                    after_sequence,
                    after_ordinal,
                    page.limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows)
    }

    pub async fn list_codex_usage_observations(
        &self,
        mut page: CodexEffectPage,
    ) -> Result<Vec<CodexProjectionEffect>, PersistError> {
        page.kind = Some("usage".into());
        self.list_codex_projection_effects(page).await
    }

    pub async fn list_codex_lifecycle_observations(
        &self,
        mut page: CodexEffectPage,
    ) -> Result<Vec<CodexProjectionEffect>, PersistError> {
        page.kind = Some("lifecycle".into());
        self.list_codex_projection_effects(page).await
    }

    pub async fn codex_projection_application(
        &self,
        generation_id: Uuid,
        event_id: i64,
    ) -> Result<Option<CodexProjectionApplication>, PersistError> {
        self.validate_codex_generation_read(generation_id).await?;
        let row = sqlx::query_as!(
            ApplicationRow,
            r#"SELECT generation_id, event_id, capture_sequence, status, issues::text AS "issues!",
                      effect_count
                 FROM runtime.codex_projection_applications WHERE generation_id = $1 AND event_id = $2"#,
            generation_id,
            event_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn list_codex_projection_issues(
        &self,
        generation_id: Uuid,
        after_sequence: i64,
        limit: i64,
    ) -> Result<Vec<CodexProjectionApplication>, PersistError> {
        validate_page(limit)?;
        self.validate_codex_generation_read(generation_id).await?;
        let rows = sqlx::query_as!(
            ApplicationRow,
            r#"SELECT generation_id, event_id, capture_sequence, status, issues::text AS "issues!",
                      effect_count
                 FROM runtime.codex_projection_applications
                WHERE generation_id = $1 AND capture_sequence > $2 AND json_array_length(issues) > 0
                ORDER BY capture_sequence
                LIMIT $3"#,
            generation_id,
            after_sequence,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

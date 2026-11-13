use chrono::Utc;
use sqlx::PgPool;

use super::PersistError;
use super::work_items::BackfillCounts;
use super::work_items_backfill;
use crate::work_items::attention;
use crate::work_items::ledger::{self, GitOutputRow, Ledger, WorkItemRow};
use crate::work_items::parse;

#[path = "work_items_delta.rs"]
pub mod work_items_delta;

async fn fetch_ledger(
    pool: &PgPool,
    session_db_id: i64,
    session_uuid: &str,
) -> Result<Ledger, PersistError> {
    let items = sqlx::query_file_as!(
        WorkItemRow,
        "src/store/persistence/work_items_ledger_rows.sql",
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    let git = sqlx::query_file!(
        "src/store/persistence/work_items_git_output.sql",
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    let git_rows: Vec<GitOutputRow> = git
        .into_iter()
        .map(|row| GitOutputRow {
            agent_id: row.agent_id,
            at: row.created_at,
            text: parse::result_text(&row.content),
        })
        .collect();
    let mut built = ledger::build_ledger(session_uuid, &items, &git_rows);
    let main_last = sqlx::query_scalar!(
        "SELECT max(r.created_at)
           FROM requests.requests r
           JOIN requests.turn_origins torg ON torg.id = r.turn_origin_id
          WHERE r.session_id = $1
            AND r.claude_code_agent_id IS NULL
            AND torg.code IN ('primary', 'primary_first')",
        session_db_id,
    )
    .fetch_one(pool)
    .await?;
    let clock = attention::Clock {
        now: Utc::now(),
        main_last_request_at: main_last,
    };
    built.attention = attention::assess(&built, &clock);
    Ok(built)
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn work_item_ledger(
        self,
        session_db_id: i64,
        session_uuid: &str,
    ) -> Result<Ledger, PersistError> {
        crate::metrics::time_db(
            "work_item_ledger",
            fetch_ledger(&self.pool, session_db_id, session_uuid),
        )
        .await
    }

    pub async fn backfill_work_items(
        self,
        session_db_id: i64,
    ) -> Result<BackfillCounts, PersistError> {
        crate::metrics::time_db(
            "backfill_work_items",
            work_items_backfill::backfill_session(self.background_pool(), session_db_id),
        )
        .await
    }
}

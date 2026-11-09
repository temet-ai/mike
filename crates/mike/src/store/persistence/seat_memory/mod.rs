use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatMemoryRefresh {
    pub items: i64,
    pub written: i64,
    pub removed: i64,
}

impl PersistHandle {
    pub async fn seat_memory_generation(&self) -> Result<i64, PersistError> {
        crate::metrics::time_db("seat_memory_generation", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT generation AS "generation!"
                     FROM memory.change_generations
                    WHERE name = 'seat_memory_sources'"#
            )
            .fetch_optional(self.pool())
            .await?
            .unwrap_or(0))
        })
        .await
    }

    pub async fn refresh_seat_memory_items(&self) -> Result<SeatMemoryRefresh, PersistError> {
        crate::metrics::time_db("refresh_seat_memory_items", async move {
            let row = sqlx::query_file!("src/store/persistence/seat_memory/refresh.sql")
                .fetch_one(self.pool())
                .await?;
            Ok(SeatMemoryRefresh {
                items: row.items,
                written: row.written,
                removed: row.removed,
            })
        })
        .await
    }
}

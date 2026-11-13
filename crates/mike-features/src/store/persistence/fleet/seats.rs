use super::{FleetNoteRow, FleetSeatRow};
use crate::store::persistence::PersistError;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn fleet_seat_rows(self, run_id: &str) -> Result<Vec<FleetSeatRow>, PersistError> {
        crate::metrics::time_db("launch_fleet_seat_rows", async move {
            let rows = sqlx::query_file_as!(
                FleetSeatRow,
                "src/store/persistence/fleet/fleet_seat_rows.sql",
                run_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fleet_note_rows(self, run_id: &str) -> Result<Vec<FleetNoteRow>, PersistError> {
        crate::metrics::time_db("launch_fleet_note_rows", async move {
            let rows = sqlx::query_file_as!(
                FleetNoteRow,
                "src/store/persistence/fleet/fleet_note_rows.sql",
                run_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}

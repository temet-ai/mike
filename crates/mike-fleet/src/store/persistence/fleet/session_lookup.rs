use uuid::Uuid;

use super::{FleetHeaderRow, FleetRow};
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

struct FleetSessionMemo(Memo<Option<FleetRow>>);

impl Default for FleetSessionMemo {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[RowChangedTable::LaunchFleets],
            per_session: &[RowChangedTable::LaunchLaunches, RowChangedTable::Sessions],
        }))
    }
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn find_fleet_for_session(
        self,
        session_uuid: Uuid,
    ) -> Result<Option<FleetRow>, PersistError> {
        let fleets = self.memo::<FleetSessionMemo>();
        let memo = self.feed_memos.bound(&fleets.0);
        if let Some(found) = memo.as_ref().and_then(|memo| memo.get(session_uuid)) {
            return Ok(found);
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let found = self.read_fleet_for_session(session_uuid).await?;
        if let (Some(memo), Some(ticket)) = (memo, ticket)
            && (found.is_some() || self.session_row_known(session_uuid))
        {
            memo.remember(session_uuid, ticket, found.clone());
        }
        Ok(found)
    }

    pub async fn find_fleet_header_for_session(
        self,
        session_uuid: Uuid,
    ) -> Result<Option<FleetHeaderRow>, PersistError> {
        let fleets = self.memo::<FleetSessionMemo>();
        if let Some(found) = self
            .feed_memos
            .bound(&fleets.0)
            .and_then(|memo| memo.get(session_uuid))
        {
            return Ok(found.as_ref().map(FleetHeaderRow::from));
        }
        crate::metrics::time_db("launch_find_fleet_header_for_session", async move {
            let row = sqlx::query_file_as!(
                FleetHeaderRow,
                "src/store/persistence/fleet/find_fleet_header_for_session.sql",
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(row) => Ok(Some(row)),
                None => Ok(sqlx::query_file_as!(
                    FleetHeaderRow,
                    "src/store/persistence/fleet/find_fleet_header_for_acceptance.sql",
                    session_uuid,
                )
                .fetch_optional(&self.pool)
                .await?),
            }
        })
        .await
    }

    async fn read_fleet_for_session(
        self,
        session_uuid: Uuid,
    ) -> Result<Option<FleetRow>, PersistError> {
        crate::metrics::time_db("launch_find_fleet_for_session", async move {
            let row = sqlx::query_file_as!(
                FleetRow,
                "src/store/persistence/fleet/find_fleet_for_session.sql",
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(row) => Ok(Some(row)),
                None => Ok(sqlx::query_file_as!(
                    FleetRow,
                    "src/store/persistence/fleet/find_fleet_for_acceptance.sql",
                    session_uuid,
                )
                .fetch_optional(&self.pool)
                .await?),
            }
        })
        .await
    }
}

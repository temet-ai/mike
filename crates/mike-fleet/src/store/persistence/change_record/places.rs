use dashmap::DashMap;

use crate::store::persistence::PersistError;

const PLACE_CAPACITY: usize = 4096;

#[derive(Debug, Default)]
pub struct PlaceMemo(DashMap<(i64, String), i64>);

impl PlaceMemo {
    pub fn get(&self, session: i64, checkout: &str) -> Option<i64> {
        self.0.get(&(session, checkout.to_owned())).map(|p| *p)
    }

    pub fn remember(&self, session: i64, checkout: &str, place: i64) {
        let key = (session, checkout.to_owned());
        if self.0.len() >= PLACE_CAPACITY && !self.0.contains_key(&key) {
            self.0.clear();
        }
        self.0
            .entry(key)
            .and_modify(|seen| *seen = (*seen).max(place))
            .or_insert(place);
    }
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn change_place(self, session: i64, checkout: &str) -> Result<i64, PersistError> {
        if let Some(place) = self.memo::<PlaceMemo>().get(session, checkout) {
            return Ok(place);
        }
        let place = crate::metrics::time_db("change_record_place", async move {
            let place = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/change_place.sql",
                session,
                checkout,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok::<_, PersistError>(place.unwrap_or(0))
        })
        .await?;
        if place > 0 {
            self.memo::<PlaceMemo>().remember(session, checkout, place);
        }
        Ok(place)
    }

    pub async fn move_change_place(
        self,
        session: i64,
        checkout: &str,
        to: i64,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("change_record_move_place", async move {
            sqlx::query_file!(
                "src/store/persistence/change_record/move_change_place.sql",
                session,
                checkout,
                to,
            )
            .execute(&self.pool)
            .await?;
            Ok::<_, PersistError>(())
        })
        .await?;
        self.memo::<PlaceMemo>().remember(session, checkout, to);
        Ok(())
    }
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod places_tests;

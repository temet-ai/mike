use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AdoptedLaunch {
    pub id: i64,
    pub seat: Option<String>,
    pub session_uuid: Option<Uuid>,
}

const ADOPT_LIVE_LAUNCHES: &str = "
WITH live AS (
    SELECT l.id
      FROM launch.launches l
     WHERE l.run_id = $1
       AND NOT l.opened
       AND l.closed_at IS NULL
       AND EXISTS (
           SELECT 1
             FROM sessions.sessions s
             JOIN runtime.model_call_facts r ON r.session_id = s.id
            WHERE s.session_uuid = l.session_uuid
              AND r.created_at >= l.created_at - interval '1 hour'
       )
)
UPDATE launch.launches l
   SET opened = true,
       opened_at = COALESCE(l.opened_at, now())
  FROM live
 WHERE l.id = live.id
RETURNING l.id, l.seat, l.session_uuid
";

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn adopt_live_fleet_launches(
        self,
        run_id: &str,
    ) -> Result<Vec<AdoptedLaunch>, PersistError> {
        crate::metrics::time_db("launch_adopt_live_fleet_launches", async move {
            Ok(
                sqlx::query_as::<_, AdoptedLaunch>(ADOPT_LIVE_LAUNCHES) // grader-allow: UPDATE across launch, sessions and runtime schemas with no .sqlx entry
                    .bind(run_id)
                    .fetch_all(&self.pool)
                    .await?,
            )
        })
        .await
    }
}

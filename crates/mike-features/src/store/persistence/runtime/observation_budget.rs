use sqlx::PgConnection;

use super::ObservationLimits;
use crate::runtime::{SessionLifecycle, SessionObservation};

pub async fn admit(
    connection: &mut PgConnection,
    session: i64,
    observation: &SessionObservation,
    limits: &ObservationLimits,
) -> Result<bool, sqlx::Error> {
    let used = sqlx::query!(
        "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'facts'"
    )
    .fetch_one(&mut *connection)
    .await?;
    let session = sqlx::query!(
        r#"SELECT count(*) AS "rows!", COALESCE(sum(retained_bytes), 0)::bigint AS "bytes!"
           FROM (SELECT retained_bytes FROM runtime.session_identities WHERE session_id = $1
                 UNION ALL SELECT retained_bytes FROM runtime.threads WHERE session_id = $1
                 UNION ALL SELECT retained_bytes FROM runtime.session_facts WHERE session_id = $1
                 UNION ALL SELECT retained_bytes FROM runtime.observation_receipts WHERE session_id = $1
                 UNION ALL SELECT retained_bytes FROM runtime.model_calls WHERE session_id = $1
                 UNION ALL SELECT retained_bytes FROM runtime.session_items WHERE session_id = $1
                 UNION ALL SELECT retained_bytes FROM runtime.notification_targets WHERE session_id = $1) retained"#,
        session
    ).fetch_one(connection).await?;
    let terminal = matches!(
        observation.lifecycle,
        Some(SessionLifecycle::Idle | SessionLifecycle::Ended | SessionLifecycle::Failed)
    );
    let row_limit = limits.max_rows
        - if terminal {
            0
        } else {
            (limits.max_rows / 10).min(1000)
        };
    let byte_limit = limits.max_bytes
        - if terminal {
            0
        } else {
            (limits.max_bytes / 10).min(1024 * 1024)
        };
    let session_row_limit = limits.max_session_rows
        - if terminal {
            0
        } else {
            (limits.max_session_rows / 10).min(100)
        };
    let session_byte_limit = limits.max_session_bytes
        - if terminal {
            0
        } else {
            (limits.max_session_bytes / 10).min(128 * 1024)
        };
    Ok(used.retained_rows <= row_limit
        && used.retained_bytes <= byte_limit
        && session.rows <= session_row_limit
        && session.bytes <= session_byte_limit)
}

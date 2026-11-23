use uuid::Uuid;

use crate::runtime::RuntimeKind;
use crate::store::persistence::{PersistError, PersistHandle};

pub struct RuntimeLaunchIdentity {
    pub runtime_home: String,
    pub folder: String,
}

impl PersistHandle {
    pub async fn runtime_launch_identity(
        &self,
        session: Uuid,
        runtime: RuntimeKind,
    ) -> Result<Option<RuntimeLaunchIdentity>, PersistError> {
        let row = sqlx::query!(
            r#"SELECT identity->>'runtime_home' AS "runtime_home?", folder
               FROM launch.launches
               WHERE session_uuid = $1 AND closed_at IS NULL
                 AND identity->>'runtime' = $2
               ORDER BY created_at DESC, id DESC LIMIT 1"#,
            session,
            runtime.as_str(),
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row.and_then(|row| {
            row.runtime_home.map(|runtime_home| RuntimeLaunchIdentity {
                runtime_home,
                folder: row.folder,
            })
        }))
    }
}

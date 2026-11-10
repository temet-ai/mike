use mike::store::persistence::PersistHandle;
use mike::store::persistence::project_knowledge::KnowledgeQuery;
use mike::store::test_support::{TestDb, TestDbHandle, seed_project, seed_session_turns};
use serde_json::Value;
use uuid::Uuid;

pub struct Fixture {
    db: TestDbHandle,
    pub handle: PersistHandle,
    pub path: String,
    pub project: i64,
}

pub struct Session {
    pub uuid: Uuid,
    pub id: i64,
}

impl Fixture {
    pub async fn open() -> anyhow::Result<Option<Self>> {
        let Some(test_db) = TestDb::bootstrap().await? else {
            return Ok(None);
        };
        let db = test_db.acquire().await?;
        let handle = PersistHandle::connect(&db.database_url).await?;
        let path = format!("/test/knowledge-search-{}", Uuid::new_v4().simple());
        let project = seed_project(handle.pool(), &path).await?;
        Ok(Some(Self {
            db,
            handle,
            path,
            project,
        }))
    }

    pub async fn session(&self) -> anyhow::Result<Session> {
        let uuid = Uuid::new_v4();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
             VALUES ($1, $2, NOW(), NOW()) RETURNING id",
        )
        .bind(uuid)
        .bind(self.project)
        .fetch_one(self.handle.pool())
        .await?;
        Ok(Session { uuid, id })
    }

    pub async fn turn(&self, session: &Session, prompt: &str, answer: &str) -> anyhow::Result<i64> {
        seed_session_turns(
            self.handle.pool(),
            &session.uuid.to_string(),
            self.project,
            1,
        )
        .await?;
        Ok(sqlx::query_scalar(
            "UPDATE memory.operator_turns SET user_prompt = $2, final_assistant_text = $3
              WHERE id = (SELECT max(id) FROM memory.operator_turns
                           WHERE session_id = $1 AND user_prompt IS NULL)
          RETURNING id",
        )
        .bind(session.id)
        .bind(prompt)
        .bind(answer)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub async fn verdict(
        &self,
        session: &Session,
        turn: i64,
        (unmet_ask, rationale): (&str, &str),
    ) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar(
            "INSERT INTO memory.turn_verdicts
                (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
                 confidence, rationale, unmet_ask, evidence_quality, as_of, input_text, reaction,
                 delivery, source, created_at)
             VALUES ($1, $2, 'live', 'r1', 'i1', 'test', 0.9, $3, $4, 'asserted', now(), '',
                     'corrected', 'partial', 'llm', now())
             RETURNING id",
        )
        .bind(turn)
        .bind(session.id)
        .bind(rationale)
        .bind(unmet_ask)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub async fn note(
        &self,
        session: &Session,
        kind: &str,
        (title, body): (&str, &str),
    ) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO workspace.session_notes (session_id, kind, title, body)
             VALUES ($1, $2, $3, $4) RETURNING note_uuid",
        )
        .bind(session.id)
        .bind(kind)
        .bind(title)
        .bind(body)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub async fn mail(&self, from: &Session, to: &Session, body: &str) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO mailbox.messages (message_uuid, from_session_id, to_session_id, body)
             VALUES (gen_random_uuid(), $1, $2, $3) RETURNING message_uuid",
        )
        .bind(from.id)
        .bind(to.id)
        .bind(body)
        .fetch_one(self.handle.pool())
        .await?)
    }

    pub async fn search(&self, query: KnowledgeQuery) -> anyhow::Result<Vec<Value>> {
        let found = self
            .handle
            .search_project_knowledge(&KnowledgeQuery {
                project: self.path.clone(),
                ..query
            })
            .await?;
        Ok(found["items"].as_array().cloned().unwrap_or_default())
    }

    pub async fn finish(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}

pub fn query(text: &str) -> KnowledgeQuery {
    KnowledgeQuery {
        query: text.to_owned(),
        ..KnowledgeQuery::default()
    }
}

pub fn references(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| item["reference"].as_str().map(str::to_owned))
        .collect()
}

pub fn others(item: &Value) -> Vec<String> {
    item["also"]
        .as_array()
        .map(|also| {
            also.iter()
                .filter_map(|reference| reference.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

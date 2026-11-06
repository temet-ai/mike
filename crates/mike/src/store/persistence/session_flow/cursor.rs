use super::*;
use sqlx::{Postgres, QueryBuilder};

pub(super) fn validate(
    session: Uuid,
    component: &FlowComponent,
    limit: u16,
    cursor: Option<&FlowCursor>,
) -> Result<(), FlowReadError> {
    if (*component == FlowComponent::Embeddings && cursor.is_some())
        || !(1..=500).contains(&limit)
        || cursor.is_some_and(|c| {
            c.version != 1
                || c.session_id != session
                || &c.component != component
                || c.filter != "all"
                || c.after_id <= 0
                || c.upper_id <= 0
                || c.after_id > c.upper_id
        })
    {
        return Err(FlowReadError::InvalidCursor);
    }
    Ok(())
}

enum Scope {
    Column(&'static str),
    ColumnAnd(&'static str, &'static str),
    Messages,
    Channel,
    LessonInjections,
    Consolidation,
}

impl Scope {
    fn push(&self, query: &mut QueryBuilder<Postgres>, session_id: i64, session: Uuid) {
        match self {
            Self::Column(column) => {
                query.push(*column).push(" = ").push_bind(session_id);
            }
            Self::ColumnAnd(column, filter) => {
                query
                    .push(*column)
                    .push(" = ")
                    .push_bind(session_id)
                    .push(" AND ")
                    .push(*filter);
            }
            Self::Messages => {
                query
                    .push("(m.to_session_id = ")
                    .push_bind(session_id)
                    .push(" OR m.from_session_id = ")
                    .push_bind(session_id)
                    .push(")");
            }
            Self::Channel => {
                query.push("c.local_session_uuid = ").push_bind(session);
            }
            Self::LessonInjections => {
                query
                    .push("COALESCE(i.session_id, r.session_id) = ")
                    .push_bind(session_id);
            }
            Self::Consolidation => {
                query
                    .push("(a.session_uuid = ")
                    .push_bind(session)
                    .push(" OR (a.session_uuid IS NULL AND a.session_id = ")
                    .push_bind(session_id)
                    .push("))");
            }
        }
    }
}

pub(super) struct Driver {
    from: &'static str,
    key: &'static str,
    scope: Scope,
}

impl Driver {
    pub(super) fn for_component(component: &FlowComponent) -> Result<Self, FlowReadError> {
        let (from, key, scope) = match component {
            FlowComponent::Requests => {
                ("requests.requests r", "r.id", Scope::Column("r.session_id"))
            }
            FlowComponent::Dispatches => (
                "agents.dispatches d JOIN tools.uses u ON u.id = d.tool_use_id",
                "d.id",
                Scope::Column("u.session_id"),
            ),
            FlowComponent::Tools => ("tools.uses u", "u.id", Scope::Column("u.session_id")),
            FlowComponent::Results => ("tools.results r", "r.id", Scope::Column("r.session_id")),
            FlowComponent::Messages => ("mailbox.messages m", "m.id", Scope::Messages),
            FlowComponent::WorkerStatus => (
                "remote_control.worker_status_transitions t JOIN remote_control.channels c ON c.id = t.channel_id",
                "t.id",
                Scope::Channel,
            ),
            FlowComponent::ManifestStages => {
                ("requests.requests r", "r.id", Scope::Column("r.session_id"))
            }
            FlowComponent::LessonInjections => (
                "memory.lesson_injections i JOIN requests.requests r ON r.id = i.request_id",
                "i.id",
                Scope::LessonInjections,
            ),
            FlowComponent::VerifierEvaluations => (
                "steering.evaluations e",
                "e.id",
                Scope::Column("e.session_id"),
            ),
            FlowComponent::Judgments => (
                "session_judges.verdicts v",
                "v.id",
                Scope::ColumnAnd("v.session_id", "v.mode = 'live'"),
            ),
            FlowComponent::LlmCalls => ("llm.calls c", "c.id", Scope::Column("c.session_id")),
            FlowComponent::Consolidation => (
                "memory.consolidation_actions a",
                "a.id",
                Scope::Consolidation,
            ),
            FlowComponent::Interventions => (
                "steering.interventions i",
                "i.id",
                Scope::Column("i.session_id"),
            ),
            FlowComponent::Embeddings => return Err(FlowReadError::InvalidCursor),
        };
        Ok(Self { from, key, scope })
    }

    pub(super) async fn window(
        &self,
        tx: &mut sqlx::PgConnection,
        session_id: i64,
        session: Uuid,
        limit: u16,
        cursor: Option<&FlowCursor>,
    ) -> Result<Window, PersistError> {
        let upper = match cursor {
            Some(c) => Some(c.upper_id),
            None => {
                let mut query = QueryBuilder::<Postgres>::new("SELECT max(");
                query
                    .push(self.key)
                    .push(") FROM ")
                    .push(self.from)
                    .push(" WHERE ");
                self.scope.push(&mut query, session_id, session);
                query
                    .build_query_scalar::<Option<i64>>()
                    .fetch_one(&mut *tx)
                    .await?
            }
        };
        let mut ids = match upper {
            Some(upper) => {
                let mut query = QueryBuilder::<Postgres>::new("SELECT ");
                query
                    .push(self.key)
                    .push(" FROM ")
                    .push(self.from)
                    .push(" WHERE ");
                self.scope.push(&mut query, session_id, session);
                query
                    .push(" AND ")
                    .push(self.key)
                    .push(" > ")
                    .push_bind(cursor.map_or(0, |c| c.after_id))
                    .push(" AND ")
                    .push(self.key)
                    .push(" <= ")
                    .push_bind(upper)
                    .push(" ORDER BY ")
                    .push(self.key)
                    .push(" LIMIT ")
                    .push_bind(i64::from(limit) + 1);
                query.build_query_scalar::<i64>().fetch_all(tx).await?
            }
            None => Vec::new(),
        };
        let exhausted = ids.len() <= usize::from(limit);
        if !exhausted {
            ids.pop();
        }
        Ok(Window {
            ids,
            upper,
            exhausted,
        })
    }
}

pub(super) struct Window {
    pub(super) ids: Vec<i64>,
    upper: Option<i64>,
    exhausted: bool,
}

impl Window {
    pub(super) fn apply(self, page: &mut FlowPage) {
        page.coverage.lower_id = self.ids.first().copied().map(FlowRowId::Bigint);
        page.coverage.upper_id = self.upper.map(FlowRowId::Bigint);
        page.coverage.returned_through_id = self.ids.last().copied().map(FlowRowId::Bigint);
        page.coverage.exhausted = self.exhausted;
        if !self.exhausted
            && let (Some(after_id), Some(upper_id)) = (self.ids.last(), self.upper)
        {
            page.next_cursor = Some(FlowCursor {
                version: 1,
                session_id: page.session_id,
                component: page.coverage.component.clone(),
                filter: "all".into(),
                after_id: *after_id,
                upper_id,
            });
        }
    }
}

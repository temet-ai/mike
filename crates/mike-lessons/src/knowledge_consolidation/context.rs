use super::decide::Stmt;
use super::items::Env;
use super::llm::Failure;
use crate::store::persistence::consolidation::{DueSubject, EvidenceChunk, NearStatement, StatementDetail};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::HashMap;

pub const NEARBY_K: i64 = 10;
pub const NEARBY_FLOOR: f64 = 0.45;
pub const SKIP_NEARBY: f64 = 0.55;
pub const SKIP_EVIDENCE: f64 = 0.60;
pub const EVIDENCE_K: i64 = 6;

pub fn stmt_of(detail: StatementDetail) -> Stmt {
    let lesson = detail.kind == "lesson";
    let always_loaded = super::reach::is_always_loaded(
        detail.source_name.as_deref(),
        detail.relative_path.as_deref(),
        detail.head.as_deref(),
    );
    Stmt {
        id: detail.statement_id,
        item_id: detail.item_id,
        kind: detail.kind,
        source_name: detail.source_name,
        carrier: detail.carrier_reference,
        text: detail.text,
        passage: detail.passage,
        passage_start: detail.passage_start,
        passage_end: detail.passage_end,
        stated_at: detail.stated_at,
        lesson_id: lesson.then_some(detail.source_id),
        lesson_active: lesson && detail.lesson_active.unwrap_or(false),
        current: detail.state == "current",
        project_id: detail.project_id,
        machine_wide: !lesson && detail.scope.as_deref() == Some("machine"),
        always_loaded,
        relative_path: detail.relative_path,
    }
}

pub fn db(error: crate::store::persistence::PersistError) -> Failure {
    Failure::transient(error.to_string())
}

pub struct Context {
    pub members: Vec<Stmt>,
    pub nearby: Vec<Stmt>,
    pub nearby_similarity: HashMap<i64, f64>,
    pub evidence: Vec<EvidenceChunk>,
}

impl Context {
    pub fn skippable(&self) -> bool {
        self.members.len() == 1
            && self.nearby_similarity.values().all(|s| *s < SKIP_NEARBY)
            && self.evidence.iter().all(|e| e.similarity < SKIP_EVIDENCE)
    }
}

pub async fn load_context<L, E>(
    env: &Env<'_, L, E>,
    subject: &DueSubject,
) -> Result<(Context, Option<String>), Failure> {
    let knowledge = env.persist.knowledge();
    let member_ids: Vec<i64> = knowledge
        .knowledge_subject_statements(subject.id)
        .await
        .map_err(db)?
        .into_iter()
        .map(|s| s.statement_id)
        .collect();
    let members: Vec<Stmt> = knowledge
        .statement_details(&member_ids)
        .await
        .map_err(db)?
        .into_iter()
        .map(stmt_of)
        .collect();
    let centre = knowledge
        .knowledge_subject_centre(subject.id)
        .await
        .map_err(db)?;
    let Some(centre_text) = centre.as_deref().filter(|_| !members.is_empty()) else {
        return Ok((empty_context(members), None));
    };
    let near: Vec<NearStatement> = knowledge
        .nearest_statements(subject.project_id, centre_text, &member_ids, NEARBY_K)
        .await
        .map_err(db)?
        .into_iter()
        .filter(|n| n.similarity >= NEARBY_FLOOR)
        .collect();
    let near_ids: Vec<i64> = near.iter().map(|n| n.statement_id).collect();
    let mut nearby: Vec<Stmt> = knowledge
        .statement_details(&near_ids)
        .await
        .map_err(db)?
        .into_iter()
        .map(stmt_of)
        .collect();
    let similarity: HashMap<i64, f64> = near.iter().map(|n| (n.statement_id, n.similarity)).collect();
    nearby.sort_by(|a, b| {
        let key = |s: &Stmt| similarity.get(&s.id).copied().unwrap_or(0.0);
        key(b).total_cmp(&key(a))
    });
    let evidence = match subject.project_id {
        Some(project) => knowledge
            .nearest_evidence_chunks(project, centre_text, EVIDENCE_K)
            .await
            .map_err(db)?,
        None => Vec::new(),
    };
    Ok((
        Context {
            members,
            nearby,
            nearby_similarity: similarity,
            evidence,
        },
        centre,
    ))
}

fn empty_context(members: Vec<Stmt>) -> Context {
    Context {
        members,
        nearby: Vec::new(),
        nearby_similarity: HashMap::new(),
        evidence: Vec::new(),
    }
}


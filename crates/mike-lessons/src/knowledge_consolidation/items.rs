use super::group::{NewlyStated, group_statements};
use super::guard::Guard;
use super::llm::{ConsolidationLlm, Embedder, Failure};
use super::passage::{SECTION_CHARS, covered_share, locate, sections};
use super::answer::RawStatement;
use super::settings::Settings;
use super::tally::Tally;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::consolidation::{ClaimedItem, NewStatement, NewStatements};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::Utc;
use std::collections::BTreeSet;
use std::time::Instant;

pub const GUIDANCE_SOURCES: [&str; 6] = [
    "claude-memory",
    "skills",
    "rules",
    "agents",
    "agent-memory",
    "instructions",
];
pub const EVIDENCE_KINDS: [&str; 6] = [
    "correction",
    "unmet_ask",
    "learning",
    "question_answer",
    "note",
    "thread_entry",
];
pub const MIN_FOUND_SHARE: f64 = 0.60;
pub const EVIDENCE_SIMILARITY: f64 = 0.60;
const SHORT_BODY_CHARS: usize = 300;
const EVIDENCE_CHUNKS: usize = 12;

pub struct Env<'a, L, E> {
    pub persist: &'a PersistHandle,
    pub llm: &'a L,
    pub embedder: &'a E,
    pub settings: &'a Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub fn locate_all(body: &str, raw: &[RawStatement]) -> Vec<Located> {
    raw.iter()
        .filter_map(|statement| {
            let text = statement.text.trim();
            if text.is_empty() {
                return None;
            }
            let found = locate(body, &statement.passage)?;
            Some(Located {
                text: text.to_owned(),
                start: found.start,
                end: found.end,
            })
        })
        .collect()
}

fn db(error: crate::store::persistence::PersistError) -> Failure {
    Failure::transient(error.to_string())
}

pub async fn process_items<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
) -> Result<(), Failure> {
    let claimed = env
        .persist
        .knowledge()
        .claim_due_knowledge_items(env.settings.items_per_tick)
        .await
        .map_err(db)?;
    for item in claimed {
        if guard.open(Instant::now()) {
            break;
        }
        match process_item(env, guard, tally, &item).await {
            Ok(()) => {
                guard.forget(('i', item.item_id));
                tally.items_done += 1;
            }
            Err(failure) => defer(env, guard, tally, &item, &failure).await,
        }
    }
    Ok(())
}

async fn defer<L, E>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    item: &ClaimedItem,
    failure: &Failure,
) {
    guard.failed(failure, Instant::now());
    let retry_at = guard.retry_at(('i', item.item_id), failure, Utc::now());
    tally.items_deferred += 1;
    tally.error(format!("{}: {}", item.reference, failure.message));
    tracing::warn!(
        reference = %item.reference,
        transient = failure.transient,
        error = %failure.message,
        "knowledge consolidation: item deferred"
    );
    save_retry(env, item.item_id, retry_at).await;
}

async fn save_retry<L, E>(env: &Env<'_, L, E>, item_id: i64, retry_at: chrono::DateTime<Utc>) {
    let saved = env
        .persist
        .knowledge()
        .defer_knowledge_item_consolidation(item_id, retry_at)
        .await;
    if let Err(error) = saved {
        tracing::warn!(%error, "knowledge consolidation: could not defer an item");
    }
}

async fn process_item<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    item: &ClaimedItem,
) -> Result<(), Failure> {
    if item.canonical_item_id != item.item_id {
        return finish(env, item).await;
    }
    match item.kind.as_str() {
        "lesson" => lesson_statement(env, guard, tally, item).await?,
        "artifact" if is_guidance(item) => file_statements(env, guard, tally, item).await?,
        kind if EVIDENCE_KINDS.contains(&kind) => evidence(env, item).await?,
        _ => {}
    }
    finish(env, item).await
}

fn is_guidance(item: &ClaimedItem) -> bool {
    item.source_name
        .as_deref()
        .is_some_and(|name| GUIDANCE_SOURCES.contains(&name))
}

async fn finish<L, E>(env: &Env<'_, L, E>, item: &ClaimedItem) -> Result<(), Failure> {
    env.persist
        .knowledge()
        .mark_knowledge_item_consolidated(item.item_id, item.consolidate_at)
        .await
        .map_err(db)?;
    Ok(())
}

async fn retire_statements<L, E>(env: &Env<'_, L, E>, item: &ClaimedItem) -> Result<(), Failure> {
    env.persist
        .knowledge()
        .mark_item_statements_gone(item.item_id, Utc::now() + env.settings.settle)
        .await
        .map_err(db)?;
    Ok(())
}

async fn lesson_statement<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    item: &ClaimedItem,
) -> Result<(), Failure> {
    let text = item.body.as_deref().map(str::trim).unwrap_or_default();
    if !item.active || text.is_empty() {
        return retire_statements(env, item).await;
    }
    let current = env
        .persist
        .knowledge()
        .item_current_statements(item.item_id)
        .await
        .map_err(db)?;
    if current.len() == 1 && current[0].text == text {
        return Ok(());
    }
    let statements = vec![Located {
        text: text.to_owned(),
        start: 0,
        end: 0,
    }];
    store_statements(env, guard, tally, item, &statements, false).await
}

async fn file_statements<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    item: &ClaimedItem,
) -> Result<(), Failure> {
    let body = item.body.as_deref().unwrap_or_default();
    if !item.active || body.trim().is_empty() {
        return retire_statements(env, item).await;
    }
    let current = env
        .persist
        .knowledge()
        .item_current_statements(item.item_id)
        .await
        .map_err(db)?;
    if current.iter().any(|s| s.split_hash == item.source_hash) && !item.detached_statements {
        return Ok(());
    }
    if item.detached_statements {
        return super::carry::carry_over(env, guard, tally, item, body).await;
    }
    let located = split_file(env, guard, body, &[(0, body.len())]).await?;
    store_statements(env, guard, tally, item, &located, true).await
}

async fn split_with<L: ConsolidationLlm, E>(
    env: &Env<'_, L, E>,
    index: usize,
    body: &str,
    ranges: &[(usize, usize)],
) -> Result<(Vec<Located>, usize, String), Failure> {
    let mut located = Vec::new();
    let mut returned = 0;
    let mut model = String::new();
    for (from, to) in ranges {
        for (offset, section) in sections(&body[*from..*to], SECTION_CHARS) {
            let offset = offset + from;
            let (answer, used) = env.llm.split(index, section).await?;
            model = used;
            returned += answer.statements.len();
            located.extend(locate_all(section, &answer.statements).into_iter().map(|mut l| {
                l.start += offset;
                l.end += offset;
                l
            }));
        }
    }
    Ok((located, returned, model))
}

pub(super) async fn split_file<L: ConsolidationLlm, E>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    body: &str,
    ranges: &[(usize, usize)],
) -> Result<Vec<Located>, Failure> {
    let models = env.llm.split_models();
    let mut last = Failure::transient("no split model is available");
    for index in 0..models {
        if guard.open(Instant::now()) {
            break;
        }
        match split_with(env, index, body, ranges).await {
            Ok((located, returned, model)) => {
                guard.succeeded(Instant::now());
                let acceptable = if returned == 0 {
                    ranges.iter().map(|(a, b)| body[*a..*b].chars().count()).sum::<usize>() < SHORT_BODY_CHARS
                        || index + 1 == models
                } else {
                    covered_share(located.len(), returned) >= MIN_FOUND_SHARE
                };
                if acceptable {
                    return Ok(located);
                }
                last = Failure::transient(format!(
                    "{model} returned {returned} statements and only {} passages were found in the file",
                    located.len()
                ));
            }
            Err(failure) => {
                guard.failed(&failure, Instant::now());
                last = failure;
            }
        }
    }
    Err(last)
}

pub(super) struct Embedded {
    pub statements: Vec<NewStatement>,
    pub vectors: Vec<String>,
}

pub(super) async fn embed_located<L, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    located: &[Located],
    with_passages: bool,
) -> Result<Embedded, Failure> {
    let texts: Vec<String> = located.iter().map(|l| l.text.clone()).collect();
    let vectors = if texts.is_empty() {
        Vec::new()
    } else {
        match env.embedder.embed(&texts).await {
            Ok(vectors) => {
                guard.succeeded(Instant::now());
                vectors
            }
            Err(failure) => {
                guard.failed(&failure, Instant::now());
                return Err(failure);
            }
        }
    };
    let statements = located
        .iter()
        .zip(&vectors)
        .map(|(l, vector)| NewStatement {
            text: l.text.clone(),
            passage_start: with_passages.then_some(l.start as i32),
            passage_end: with_passages.then_some(l.end as i32),
            embedding: vector.clone(),
        })
        .collect();
    Ok(Embedded {
        statements,
        vectors,
    })
}

pub(super) fn new_statements<L, E: Embedder>(
    env: &Env<'_, L, E>,
    item: &ClaimedItem,
    statements: Vec<NewStatement>,
) -> NewStatements {
    NewStatements {
        split_hash: item.source_hash.clone(),
        model_key: env.embedder.model_key(),
        stated_at: item.stated_at,
        judge_after: Utc::now() + env.settings.settle,
        statements,
    }
}

pub(super) async fn group_created<L, E>(
    env: &Env<'_, L, E>,
    tally: &mut Tally,
    item: &ClaimedItem,
    created: &[crate::store::persistence::consolidation::CreatedStatement],
    vectors: &[String],
) -> Result<(), Failure> {
    tally.statements_created += created.len() as u32;
    tally.projects.insert(item.project_id);
    let fresh: Vec<NewlyStated> = created
        .iter()
        .zip(vectors)
        .map(|(created, vector)| NewlyStated {
            statement_id: created.id,
            embedding: vector.clone(),
        })
        .collect();
    let grouped = group_statements(env.persist, env.settings, item.project_id, &fresh, None).await?;
    tally.subjects_created += grouped.created;
    Ok(())
}

async fn store_statements<L: ConsolidationLlm, E: Embedder>(
    env: &Env<'_, L, E>,
    guard: &mut Guard,
    tally: &mut Tally,
    item: &ClaimedItem,
    located: &[Located],
    with_passages: bool,
) -> Result<(), Failure> {
    let embedded = embed_located(env, guard, located, with_passages).await?;
    let new = new_statements(env, item, embedded.statements);
    let replaced = env
        .persist
        .knowledge()
        .replace_item_statements(item.item_id, &new)
        .await
        .map_err(db)?;
    group_created(env, tally, item, &replaced.created, &embedded.vectors).await
}

async fn evidence<L, E>(env: &Env<'_, L, E>, item: &ClaimedItem) -> Result<(), Failure> {
    let knowledge = env.persist.knowledge();
    let chunks = knowledge
        .item_chunk_embeddings(item.item_id)
        .await
        .map_err(db)?;
    let mut subjects = BTreeSet::new();
    for chunk in chunks.iter().take(EVIDENCE_CHUNKS) {
        let near = knowledge
            .nearest_statements(item.project_id, chunk, &[], 1)
            .await
            .map_err(db)?;
        if let Some(nearest) = near.first()
            && nearest.similarity >= EVIDENCE_SIMILARITY
            && let Some(subject) = nearest.subject_id
        {
            subjects.insert(subject);
        }
    }
    if !subjects.is_empty() {
        let ids: Vec<i64> = subjects.into_iter().collect();
        knowledge
            .mark_knowledge_subjects_changed(&ids, Utc::now() + env.settings.settle)
            .await
            .map_err(db)?;
    }
    Ok(())
}

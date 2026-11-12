pub(crate) mod hook;
mod parser;

pub use crate::config::memory_harvester::HarvesterConfig;
pub(crate) use hook::HarvesterHook;
pub use parser::scan_update_tags;

use std::borrow::Cow;

use parser::UpdateTag;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::NewLessonWrite;

pub struct HarvestContext {
    pub project_id: i64,
    pub session_id: Option<i64>,
    pub request_id: Option<i64>,
    pub source_turn_id: Option<i64>,
}

#[derive(Debug, Default)]
pub struct HarvestResult {
    pub tags_found: usize,
    pub lessons_written: usize,
    pub lessons_skipped: usize,
    pub lessons_truncated: usize,
}

pub async fn harvest_from_response(
    text: &str,
    ctx: &HarvestContext,
    persist: &PersistHandle,
    cfg: &HarvesterConfig,
) -> HarvestResult {
    debug_assert!(cfg.enabled, "harvest_from_response called when disabled");

    let mut result = HarvestResult::default();

    if ctx.source_turn_id.is_none() && ctx.request_id.is_none() {
        tracing::warn!(
            project_id = ctx.project_id,
            "harvester: provenance missing (source_turn_id and request_id both None) — skipping all tags"
        );
        return result;
    }

    let tags = scan_update_tags(text);
    result.tags_found = tags.len();

    if tags.is_empty() {
        return result;
    }

    for tag in &tags {
        let body = truncate_lesson_body(tag, cfg.max_lesson_bytes, ctx.project_id, &mut result);
        write_harvested_lesson(persist, ctx, tag, &body, &mut result).await;
    }

    result
}

fn truncate_lesson_body<'a>(
    tag: &'a UpdateTag,
    max_bytes: usize,
    project_id: i64,
    result: &mut HarvestResult,
) -> Cow<'a, str> {
    if tag.text.len() <= max_bytes {
        return Cow::Borrowed(&tag.text);
    }
    let mut end = max_bytes;
    while end > 0 && !tag.text.is_char_boundary(end) {
        end -= 1;
    }
    result.lessons_truncated += 1;
    tracing::warn!(
        project_id = project_id,
        topic = %tag.topic,
        original_bytes = tag.text.len(),
        truncated_to = end,
        "harvester: lesson body truncated to max_lesson_bytes"
    );
    Cow::Owned(tag.text[..end].to_string())
}

async fn write_harvested_lesson(
    persist: &PersistHandle,
    ctx: &HarvestContext,
    tag: &UpdateTag,
    body: &str,
    result: &mut HarvestResult,
) {
    match persist
        .create_harvested_lesson(NewLessonWrite {
            project_id: ctx.project_id,
            kind: &tag.kind,
            text: body,
            topic: Some(tag.topic.as_str()),
            source_turn_id: ctx.source_turn_id,
            agent_id: None,
            session_db_id: ctx.session_id,
            request_id: ctx.request_id,
        })
        .await
    {
        Ok(Some(_)) => {
            result.lessons_written += 1;
        }
        Ok(None) => {
            debug_lesson_dedup_hit(ctx.project_id, &tag.topic);
            result.lessons_skipped += 1;
        }
        Err(e) => {
            warn_lesson_write_failed(ctx.project_id, &tag.topic, &e);
            result.lessons_skipped += 1;
        }
    }
}

fn debug_lesson_dedup_hit(project_id: i64, topic: &str) {
    tracing::debug!(
        project_id = project_id,
        topic = %topic,
        "harvester: dedup hit, lesson already exists"
    );
}

fn warn_lesson_write_failed(project_id: i64, topic: &str, e: &impl std::fmt::Display) {
    tracing::warn!(
        project_id = project_id,
        topic = %topic,
        error = %e,
        "harvester: failed to write lesson — skipping"
    );
}

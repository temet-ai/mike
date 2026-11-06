use super::*;
use crate::store::persistence::lessons::NewLessonWrite;

pub(crate) async fn create_lesson(
    State(state): State<AppState>,
    Json(mut body): Json<LessonCreate>,
) -> Result<(StatusCode, Json<CreateLessonResponse>), Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    validate_lesson_create_shape(&body).map_err(|e| *e)?;
    if let Some(project) = handle
        .find_knowledge_project(&body.project_path)
        .await
        .map_err(crate::api::project_knowledge::knowledge_error)?
    {
        body.project_path = project.project_path;
    } else if body.project_path.parse::<uuid::Uuid>().is_ok() {
        return Err(lesson_error(StatusCode::NOT_FOUND, "Unknown project UUID"));
    }
    let mut notes: Vec<String> = Vec::new();
    let supersedes = match body.supersedes {
        Some(declared) => {
            ensure_lesson_exists(handle, declared).await?;
            let heads = handle
                .live_replacements_of(declared)
                .await
                .unwrap_or_default();
            let (target, note) = resolved_supersedes(declared, &heads);
            notes.extend(note);
            Some(target)
        }
        None => None,
    };

    let project_id = handle
        .upsert_project_returning_id(&body.project_path)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "create_lesson: project upsert failed");
            persist_error_response(e)
        })?;
    let agent_id = resolve_agent_id(handle, body.agent_name.as_deref()).await?;
    let (session_db_id, resolved_source_turn_id) =
        resolve_lesson_provenance(handle, body.source_turn_id, body.session_id.as_deref()).await?;

    let max_chars = lesson_shape_limit(handle).await;
    let hold = state.feature_toggle_overlay.load().judge_lessons;
    let assessment = crate::lesson_shape::assess(&body.text, body.topic.as_deref(), max_chars);
    let row = handle
        .create_lesson_with_shape(
            NewLessonWrite {
                project_id,
                kind: &body.kind,
                text: &body.text,
                topic: body.topic.as_deref(),
                source_turn_id: resolved_source_turn_id,
                agent_id,
                session_db_id,
                request_id: None,
            },
            &assessment,
            hold,
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "create_lesson: insert failed");
            persist_error_response(e)
        })?;

    if let Some(supersedes_id) = supersedes {
        insert_supersedes_relation(handle, row.id, supersedes_id).await?;
    }

    let repo_file = None;
    let repo_warning = Some("Portable lesson synchronization is queued; run mike-knowledge sync/watch in the registered checkout.".to_string());

    let (related_sessions, neighbour_lessons) =
        fetch_related_and_neighbours(handle, body.session_id.as_deref()).await?;
    let near_lessons = near_lessons(&state, handle, project_id, &body.text, row.id).await;
    notes.extend(near_note(&near_lessons, supersedes));
    let note = (!notes.is_empty()).then(|| notes.join(" "));

    Ok((
        StatusCode::CREATED,
        Json(CreateLessonResponse {
            shape: LessonShapeDto::from_assessment(&assessment, max_chars, hold),
            repo_file,
            repo_warning,
            lesson: LessonDto {
                id: row.id,
                project_id: Some(project_id),
                project_path: Some(body.project_path),
                kind: body.kind,
                text: body.text,
                source_turn_id: resolved_source_turn_id,
                created_at: row.created_at.to_rfc3339(),
                expires_at: row.expires_at.map(|t| t.to_rfc3339()),
                topic: body.topic,
            },
            related_sessions,
            neighbour_lessons,
            near_lessons,
            note,
        }),
    ))
}

pub(crate) fn resolved_supersedes(declared: i64, live_heads: &[i64]) -> (i64, Option<String>) {
    match live_heads {
        [] => (declared, None),
        [head] => (
            *head,
            Some(format!(
                "Lesson {declared} had already been replaced by lesson {head}, so this lesson is recorded as superseding {head}."
            )),
        ),
        heads => (
            declared,
            Some(format!(
                "Lesson {declared} was split into lessons {}; this lesson is recorded as superseding the original. If it replaces one of those, record it again with that id in `supersedes`.",
                heads
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        ),
    }
}

pub(crate) fn near_note(near: &[NearLessonDto], supersedes: Option<i64>) -> Option<String> {
    let others: Vec<String> = near
        .iter()
        .filter(|n| Some(n.id) != supersedes)
        .map(|n| format!("{} ({:.0}%)", n.id, n.similarity * 100.0))
        .collect();
    if others.is_empty() {
        return None;
    }
    Some(format!(
        "{} active lesson{} say{} nearly the same thing: {}. If yours replaces one, record it again with that id in `supersedes`; if it only repeats one, delete yours with delete_lesson; if it adds something, nothing more is needed.",
        others.len(),
        if others.len() == 1 { "" } else { "s" },
        if others.len() == 1 { "s" } else { "" },
        others.join(", ")
    ))
}

async fn near_lessons(
    state: &AppState,
    handle: &crate::store::persistence::PersistHandle,
    project_id: i64,
    text: &str,
    own_id: i64,
) -> Vec<NearLessonDto> {
    let embedding = state.embedding_service();
    if !embedding.is_available() {
        return Vec::new();
    }
    let vector = match embedding.embed(text).await {
        Ok(vector) => vector,
        Err(error) => {
            tracing::debug!(%error, "create_lesson: the text was not embedded, so no near lessons are named");
            return Vec::new();
        }
    };
    let neighbours = handle
        .find_lesson_neighbours(
            project_id,
            &vector,
            crate::lesson_proposer::relate::NEIGHBOUR_MAX_DISTANCE,
            crate::lesson_proposer::relate::NEIGHBOUR_LIMIT,
        )
        .await
        .unwrap_or_default();
    neighbours
        .into_iter()
        .filter(|n| n.id != own_id)
        .map(|n| NearLessonDto {
            id: n.id,
            text: n.text,
            similarity: f64::from(1.0 - n.distance),
            source: n.source,
        })
        .collect()
}

async fn lesson_shape_limit(handle: &crate::store::persistence::PersistHandle) -> usize {
    match handle.fetch_settings().await {
        Ok(rows) => crate::lesson_shape::max_chars_from_rows(&rows),
        Err(e) => {
            tracing::warn!(error = %e, "create_lesson: settings unavailable, using the default shape limit");
            crate::lesson_shape::DEFAULT_MAX_CHARS
        }
    }
}

fn validate_lesson_create_shape(body: &LessonCreate) -> Result<(), Box<Response>> {
    if body.project_path.trim().is_empty() {
        return Err(Box::new(lesson_error(
            StatusCode::BAD_REQUEST,
            "`project_path` must be non-empty",
        )));
    }
    if super::lesson_project_guard::ephemeral_project_path(&body.project_path) {
        return Err(Box::new(lesson_error(
            StatusCode::BAD_REQUEST,
            "`project_path` names an ephemeral directory (temp dir, scratchpad, selftest); \
             a test must not write memory into the running system",
        )));
    }
    if !LESSON_KINDS.contains(&body.kind.as_str()) {
        return Err(Box::new(lesson_error(
            StatusCode::BAD_REQUEST,
            "`kind` must be one of correction|convention|warning|preference",
        )));
    }
    if body.text.trim().is_empty() {
        return Err(Box::new(lesson_error(
            StatusCode::BAD_REQUEST,
            "`text` must be non-empty",
        )));
    }
    Ok(())
}

async fn ensure_lesson_exists(
    handle: &crate::store::persistence::PersistHandle,
    lesson_id: i64,
) -> Result<(), Response> {
    let exists = handle.lesson_exists(lesson_id).await.map_err(|e| {
        tracing::warn!(error = %e, "create_lesson: supersedes existence check failed");
        persist_error_response(e)
    })?;
    if exists {
        Ok(())
    } else {
        Err(lesson_error(
            StatusCode::BAD_REQUEST,
            "`supersedes` does not name an existing lesson",
        ))
    }
}

async fn resolve_agent_id(
    handle: &crate::store::persistence::PersistHandle,
    agent_name: Option<&str>,
) -> Result<Option<i64>, Response> {
    let Some(name) = agent_name else {
        return Ok(None);
    };
    Ok(handle
        .fetch_agent_by_name(name)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "create_lesson: agent lookup failed");
            persist_error_response(e)
        })?
        .map(|a| a.id))
}

async fn resolve_lesson_provenance(
    handle: &crate::store::persistence::PersistHandle,
    source_turn_id: Option<i64>,
    session_id: Option<&str>,
) -> Result<(Option<i64>, Option<i64>), Response> {
    if let Some(turn_id) = source_turn_id {
        let session_db_id = handle.session_id_for_turn(turn_id).await.map_err(|e| {
            tracing::warn!(error = %e, "create_lesson: session lookup failed");
            persist_error_response(e)
        })?;
        return Ok((session_db_id, Some(turn_id)));
    }
    let Some(session_uuid) = session_id else {
        return Ok((None, None));
    };
    let Some(session_db_id) = handle
        .resolve_session_db_id(session_uuid)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "create_lesson: session resolve failed");
            persist_error_response(e)
        })?
    else {
        return Err(lesson_error(
            StatusCode::BAD_REQUEST,
            "`session_id` does not match any known session",
        ));
    };
    let turn_id = handle
        .latest_turn_id_for_session(session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "create_lesson: turn lookup failed");
            persist_error_response(e)
        })?;
    Ok((Some(session_db_id), turn_id))
}

async fn insert_supersedes_relation(
    handle: &crate::store::persistence::PersistHandle,
    lesson_id: i64,
    supersedes_id: i64,
) -> Result<(), Response> {
    handle
        .insert_lesson_relation(
            &crate::store::persistence::lesson_relations::LessonRelationDraft {
                from_lesson_id: lesson_id,
                to_lesson_id: supersedes_id,
                kind: crate::lesson_relations::KIND_SUPERSEDES,
                method: crate::lesson_relations::METHOD_DECLARED,
                status: crate::lesson_relations::STATUS_CONFIRMED,
                candidate_distance: None,
                evidence_json: serde_json::json!({}),
                proposer_model: None,
                proposer_rationale: None,
                adjudicator_model: None,
                adjudicator_agrees: None,
                adjudicator_rationale: None,
                detector_version: crate::lesson_relations::DETECTOR_VERSION,
                decided_by: Some("declared"),
            },
        )
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                lesson_id,
                supersedes_id,
                "create_lesson: declared supersedes relation insert failed"
            );
            persist_error_response(e)
        })?;
    Ok(())
}

async fn fetch_related_and_neighbours(
    handle: &crate::store::persistence::PersistHandle,
    session_id: Option<&str>,
) -> Result<(Vec<LessonRelatedSessionDto>, Vec<NeighbourLessonDto>), Response> {
    let Some(session_uuid) = session_id.and_then(|s| s.parse::<uuid::Uuid>().ok()) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let neighbours = handle
        .nearest_sessions(session_uuid, 5, false, 0.0)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "create_lesson: nearest_sessions failed");
            persist_error_response(e)
        })?;
    let related: Vec<LessonRelatedSessionDto> = neighbours
        .iter()
        .map(|n| LessonRelatedSessionDto {
            session_id: n.session_id.to_string(),
            title: n.title.clone(),
            similarity: n.similarity,
        })
        .collect();
    let neighbour_ids: Vec<uuid::Uuid> = neighbours.iter().map(|n| n.session_id).collect();
    let lessons = if neighbour_ids.is_empty() {
        Vec::new()
    } else {
        handle
            .lessons_from_sessions(&neighbour_ids)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "create_lesson: neighbour lessons fetch failed");
                persist_error_response(e)
            })?
            .into_iter()
            .map(|l| NeighbourLessonDto {
                id: l.id,
                session_id: l.session_id.to_string(),
                kind: l.kind,
                stage: l.stage,
                text: l.text,
            })
            .collect()
    };
    Ok((related, lessons))
}

#[cfg(test)]
mod supersedes_tests {
    use super::{NearLessonDto, near_note, resolved_supersedes};

    fn near(id: i64, similarity: f64) -> NearLessonDto {
        NearLessonDto {
            id,
            text: format!("lesson {id}"),
            similarity,
            source: "judge".into(),
        }
    }

    #[test]
    fn a_declared_target_already_replaced_once_is_followed_to_its_replacement() {
        assert_eq!(resolved_supersedes(2083, &[]), (2083, None));
        let (target, note) = resolved_supersedes(2083, &[2086]);
        assert_eq!(target, 2086);
        assert!(note.is_some_and(|n| n.contains("replaced by lesson 2086")));
        let (target, note) = resolved_supersedes(2004, &[2005, 2006]);
        assert_eq!(target, 2004);
        assert!(note.is_some_and(|n| n.contains("split into lessons 2005, 2006")));
    }

    #[test]
    fn the_near_note_names_the_twins_and_leaves_out_the_one_being_superseded() {
        assert_eq!(near_note(&[], None), None);
        assert_eq!(near_note(&[near(2086, 0.91)], Some(2086)), None);
        let note = near_note(&[near(2086, 0.91), near(2090, 0.80)], Some(2086)).unwrap_or_default();
        assert!(
            note.starts_with("1 active lesson says nearly the same thing: 2090 (80%)."),
            "{note}"
        );
        let note = near_note(&[near(2086, 0.91), near(2090, 0.80)], None).unwrap_or_default();
        assert!(
            note.starts_with("2 active lessons say nearly the same thing: 2086 (91%), 2090 (80%)."),
            "{note}"
        );
        assert!(note.contains("delete_lesson"), "{note}");
    }
}

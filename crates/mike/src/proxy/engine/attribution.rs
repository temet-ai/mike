use crate::state::AppState;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) fn spawn_alter_decision_stamp(
    state: &AppState,
    observed_composition_applied: bool,
    session_id: &str,
) {
    if !observed_composition_applied {
        return;
    }
    let Some(persist) = &state.persist else {
        return;
    };
    let persist_for_alter = persist.clone();
    let session_id_for_alter = session_id.to_string();
    crate::observability::spans::spawn_detached(async move {
        if let Err(e) = persist_for_alter
            .mark_session_prompt_altered(&session_id_for_alter)
            .await
        {
            tracing::warn!(
                session_id = %session_id_for_alter,
                error = %e,
                "ledger main-row altered stamp failed — will retry on the session's next Templated turn"
            );
        }
    });
}

pub(super) fn spawn_lesson_injection_attribution(
    state: &AppState,
    session_id: &str,
    lesson_injection_lane: &'static str,
    lesson_injection_persona: Option<String>,
    injected_lesson_ids: Vec<i64>,
    request_id: RequestId,
) {
    if injected_lesson_ids.is_empty() {
        return;
    }
    let Some(persist) = &state.persist else {
        return;
    };
    let persist_for_lessons = persist.clone();
    let session_id_for_lessons = session_id.to_string();
    let db_request_id = request_id.as_u64() as i64;
    crate::observability::spans::spawn_detached(async move {
        if let Err(e) = persist_for_lessons
            .lessons()
            .record_lesson_injections(
                db_request_id,
                &session_id_for_lessons,
                lesson_injection_lane,
                lesson_injection_persona.as_deref(),
                &injected_lesson_ids,
            )
            .await
        {
            tracing::warn!(
                request_id = db_request_id,
                error = %e,
                "lesson injection attribution write failed"
            );
        }
    });
}

pub(super) fn spawn_mod_lesson_attribution(
    state: &AppState,
    session_id: &str,
    lesson_injection_lane: &'static str,
    mod_lesson_ids: Vec<i64>,
    request_id: RequestId,
) {
    if mod_lesson_ids.is_empty() {
        return;
    }
    let Some(persist) = &state.persist else {
        return;
    };
    let persist = persist.clone();
    let session_id = session_id.to_string();
    let db_request_id = request_id.as_u64() as i64;
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(e) = persist
            .lessons()
            .record_mod_lesson_injections(
                db_request_id,
                &session_id,
                lesson_injection_lane,
                &mod_lesson_ids,
            )
            .await
        {
            tracing::warn!(
                request_id = db_request_id,
                error = %e,
                "mod lesson attribution write failed"
            );
        }
    });
}

pub(super) async fn stamp_steerer_sent(
    state: &AppState,
    request_id: RequestId,
    injected_reminders: &[SystemReminder],
) {
    if injected_reminders.is_empty() {
        return;
    }
    let Some(persist) = &state.persist else {
        return;
    };
    let db_request_id = request_id.as_u64() as i64;
    for reminder in injected_reminders {
        stamp_one_reminder_sent(persist, request_id, db_request_id, reminder).await;
    }
}

async fn stamp_one_reminder_sent(
    persist: &PersistHandle,
    request_id: RequestId,
    db_request_id: i64,
    reminder: &SystemReminder,
) {
    match &reminder.source {
        ReminderSource::Steerer {
            kind,
            intervention_uuid,
            ..
        } => {
            stamp_steerer_source_sent(persist, request_id, db_request_id, kind, intervention_uuid)
                .await;
        }
        ReminderSource::SessionMailbox { message_uuid, .. } => {
            stamp_mailbox_source_sent(persist, request_id, db_request_id, *message_uuid).await;
        }
        ReminderSource::Diagnostic
        | ReminderSource::Lesson
        | ReminderSource::Recall
        | ReminderSource::SelfIdentity
        | ReminderSource::WorkspaceBriefing => {}
    }
}

async fn stamp_steerer_source_sent(
    persist: &PersistHandle,
    request_id: RequestId,
    db_request_id: i64,
    kind: &str,
    intervention_uuid: &str,
) {
    let sent = persist
        .steerer()
        .steering_backfill_sent_request_id(intervention_uuid, db_request_id)
        .await;
    log_steerer_sent_stamp_result(request_id, kind, intervention_uuid, sent);

    let trigger = persist
        .steerer()
        .steering_backfill_trigger_request_id(intervention_uuid, db_request_id)
        .await;
    log_steerer_trigger_backfill_result(request_id, kind, intervention_uuid, trigger);
}

fn log_steerer_sent_stamp_result(
    request_id: RequestId,
    kind: &str,
    intervention_uuid: &str,
    result: Result<bool, PersistError>,
) {
    match result {
        Ok(true) => log_steerer_sent_stamped(request_id, kind, intervention_uuid),
        Ok(false) => log_steerer_sent_no_match(request_id, kind, intervention_uuid),
        Err(e) => log_steerer_sent_failed(request_id, kind, intervention_uuid, e),
    }
}

fn log_steerer_sent_stamped(request_id: RequestId, kind: &str, intervention_uuid: &str) {
    tracing::debug!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        "steering reminder sent_request_id stamped"
    );
}

fn log_steerer_sent_no_match(request_id: RequestId, kind: &str, intervention_uuid: &str) {
    tracing::trace!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        "steering reminder sent stamp: no matching intervention row"
    );
}

fn log_steerer_sent_failed(
    request_id: RequestId,
    kind: &str,
    intervention_uuid: &str,
    e: PersistError,
) {
    tracing::warn!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        error = %e,
        "steering reminder sent stamp failed"
    );
}

fn log_steerer_trigger_backfill_result(
    request_id: RequestId,
    kind: &str,
    intervention_uuid: &str,
    result: Result<bool, PersistError>,
) {
    match result {
        Ok(true) => log_steerer_trigger_backfilled(request_id, kind, intervention_uuid),
        Ok(false) => {}
        Err(e) => log_steerer_trigger_backfill_failed(request_id, kind, intervention_uuid, e),
    }
}

fn log_steerer_trigger_backfilled(request_id: RequestId, kind: &str, intervention_uuid: &str) {
    tracing::debug!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        "same-request steering fire: trigger_request_id backfilled"
    );
}

fn log_steerer_trigger_backfill_failed(
    request_id: RequestId,
    kind: &str,
    intervention_uuid: &str,
    e: PersistError,
) {
    tracing::warn!(
        %request_id,
        %intervention_uuid,
        kind = %kind,
        error = %e,
        "same-request steering fire: trigger_request_id backfill failed"
    );
}

async fn stamp_mailbox_source_sent(
    persist: &PersistHandle,
    request_id: RequestId,
    db_request_id: i64,
    message_uuid: uuid::Uuid,
) {
    let result = match persist
        .launch()
        .backfill_mailbox_sent_request_id(message_uuid, db_request_id)
        .await
    {
        Err(first) => {
            tracing::warn!(
                %request_id,
                %message_uuid,
                error = %first,
                "mailbox delivery back-fill failed; stamping the message sent without its request so it is not left claimed"
            );
            persist
                .launch()
                .stamp_mailbox_sent_without_request(message_uuid)
                .await
        }
        stamped => stamped,
    };
    log_mailbox_sent_stamp_result(request_id, message_uuid, result);
}

fn log_mailbox_sent_stamp_result(
    request_id: RequestId,
    message_uuid: uuid::Uuid,
    result: Result<bool, PersistError>,
) {
    match result {
        Ok(true) => log_mailbox_sent_backfilled(request_id, message_uuid),
        Ok(false) => log_mailbox_sent_no_match(request_id, message_uuid),
        Err(e) => log_mailbox_sent_failed(request_id, message_uuid, e),
    }
}

fn log_mailbox_sent_backfilled(request_id: RequestId, message_uuid: uuid::Uuid) {
    tracing::debug!(%request_id, %message_uuid, "mailbox delivery back-filled");
}

fn log_mailbox_sent_no_match(request_id: RequestId, message_uuid: uuid::Uuid) {
    tracing::debug!(
        %request_id,
        %message_uuid,
        "mailbox delivery back-fill: no matching undelivered row"
    );
}

fn log_mailbox_sent_failed(request_id: RequestId, message_uuid: uuid::Uuid, e: PersistError) {
    tracing::warn!(
        %request_id,
        %message_uuid,
        error = %e,
        "mailbox delivery back-fill failed"
    );
}

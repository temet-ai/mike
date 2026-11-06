use crate::extract::request::TurnOrigin;
use crate::lesson_worker::LessonSnapshot;
use crate::proxy::remote_control::inject::PushDeps;
use crate::store::models::{ReminderSource, RequestId};

use super::{RemindersOutcome, push_or_append_reminder};

pub(crate) struct AgentLessonsRequest<'a> {
    pub(crate) body_bytes: &'a [u8],
    pub(crate) session_id: &'a str,
    pub(crate) subagent_persona: Option<&'a str>,
    pub(crate) turn_origin: &'a TurnOrigin,
    pub(crate) snapshot: &'a LessonSnapshot,
    pub(crate) agent_lessons_enabled: bool,
    pub(crate) request_id: RequestId,
    pub(crate) push_deps: &'a PushDeps,
}

pub(crate) async fn apply_agent_lessons(req: AgentLessonsRequest<'_>) -> RemindersOutcome {
    let AgentLessonsRequest {
        body_bytes,
        session_id,
        subagent_persona,
        turn_origin,
        snapshot,
        agent_lessons_enabled,
        request_id,
        push_deps,
    } = req;

    if !agent_lessons_enabled {
        return RemindersOutcome::Unchanged;
    }

    if !matches!(turn_origin, TurnOrigin::Subagent) {
        return RemindersOutcome::Unchanged;
    }

    let persona = subagent_persona.filter(|p| !p.is_empty());
    let resolved = persona.and_then(|p| Some((p, snapshot.resolve_persona(p)?)));
    let served_by_client = persona
        .is_some_and(|p| snapshot.agent_lesson_served_by_client(session_id, p))
        || resolved.is_none() && snapshot.session_agent_lessons_served_by_client(session_id);
    if served_by_client {
        return RemindersOutcome::Suppressed {
            reason: "served_by_client",
        };
    }

    let Some((persona, agent_id)) = resolved else {
        return RemindersOutcome::Unchanged;
    };

    let lessons = snapshot.agent_lessons_for(agent_id);
    if lessons.is_empty() {
        return RemindersOutcome::Unchanged;
    }

    if !snapshot.mark_agent_lesson_injected(session_id, persona) {
        return RemindersOutcome::Unchanged;
    }

    let injectable: Vec<(String, String)> = lessons
        .iter()
        .map(|l| (l.kind.clone(), l.text.clone()))
        .collect();
    let Some(reminder_text) = build_agent_lessons_reminder_text(persona, &injectable) else {
        return RemindersOutcome::Unchanged;
    };

    push_or_append_reminder(super::PushOrAppendRequest {
        push_deps,
        session_id,
        body_bytes,
        source: ReminderSource::Lesson,
        reminder_text,
        source_finding_ids: lessons.iter().map(|l| l.id).collect(),
        source_request_id: 0,
        request_id,
    })
    .await
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AgentLessonsAnswer {
    pub(crate) agent_id: Option<i64>,
    pub(crate) lesson_ids: Vec<i64>,
    pub(crate) text: Option<String>,
}

pub(crate) fn agent_lessons_answer(
    snapshot: &LessonSnapshot,
    session_id: &str,
    agent_type: &str,
) -> AgentLessonsAnswer {
    let Some(agent_id) = snapshot.resolve_persona(agent_type) else {
        return AgentLessonsAnswer {
            agent_id: None,
            lesson_ids: Vec::new(),
            text: None,
        };
    };
    snapshot.mark_agent_lesson_served_by_client(session_id, agent_type);
    let lessons = snapshot.agent_lessons_for(agent_id);
    let injectable: Vec<(String, String)> = lessons
        .iter()
        .map(|l| (l.kind.clone(), l.text.clone()))
        .collect();
    AgentLessonsAnswer {
        agent_id: Some(agent_id),
        lesson_ids: lessons.iter().map(|l| l.id).collect(),
        text: build_agent_lessons_reminder_text(agent_type, &injectable),
    }
}

fn build_agent_lessons_reminder_text(
    persona: &str,
    lessons: &[(String, String)],
) -> Option<String> {
    if lessons.is_empty() {
        return None;
    }
    let label = sanitize_persona_label(persona);
    let mut out = String::new();
    out.push_str("<system-reminder source=\"proxy-agent-lessons\">\n");
    out.push_str(&format!(
        "Lessons recorded for agent \"{label}\" from prior sessions:\n"
    ));
    for (kind, text) in lessons {
        let kind_s = sanitize_persona_label(kind);
        out.push_str("- [");
        out.push_str(&kind_s);
        out.push_str("] ");
        out.push_str(text.trim());
        out.push('\n');
    }
    out.push_str("</system-reminder>");
    Some(out)
}

fn sanitize_persona_label(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

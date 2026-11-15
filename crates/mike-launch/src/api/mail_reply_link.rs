use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::base_state::BaseState;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::{head_chars, tail_chars};
use crate::store::persistence::launch_repo::LaunchPersist as _;

pub const POINT: &str = "mail_reply_link";
pub const QUESTION_SET: &str = "mail_reply_link_v1";
const WHICH: &str = "which";
const NONE: &str = "none";
const MAX_CANDIDATES: usize = 8;
const BODY_CHARS: usize = 600;
const REPLY_CHARS: usize = 1200;
const SEND_LOOKBACK_HOURS: i64 = 24;

#[derive(Debug, Deserialize)]
pub struct LinkRequest {
    pub sender: Uuid,
    pub reply: String,
    #[serde(default)]
    pub candidates: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: Uuid,
    pub queued_at: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Linked {
    Message { id: Uuid, probability: f64 },
    Nothing { probability: f64 },
    Unanswered,
}

#[derive(Debug, Serialize)]
pub struct LinkAnswer {
    pub in_reply_to: Option<Uuid>,
    pub decided_by: &'static str,
    pub probability: Option<f64>,
}

fn key(position: usize) -> String {
    format!("m{}", position + 1)
}

pub fn questions(candidates: &[Candidate]) -> BTreeMap<String, Question> {
    let mut options: Vec<(String, String)> = candidates
        .iter()
        .enumerate()
        .map(|(at, c)| {
            (
                key(at),
                format!("The reply answers the message queued at {}.", c.queued_at),
            )
        })
        .collect();
    options.push((
        NONE.to_owned(),
        "The reply answers none of these messages; it starts a new subject.".to_owned(),
    ));
    let listed: Vec<(&str, &str)> = options
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    BTreeMap::from([(
        WHICH.to_owned(),
        choice(
            "A session is replying to a colleague. Which of the colleague's earlier messages to it does the reply answer?",
            &listed,
        ),
    )])
}

pub fn state(reply: &str, candidates: &[Candidate]) -> Value {
    let messages: Vec<Value> = candidates
        .iter()
        .enumerate()
        .map(|(at, c)| {
            json!({
                "id": key(at),
                "queued_at": c.queued_at,
                "body": head_chars(&c.body, BODY_CHARS),
            })
        })
        .collect();
    json!({
        "reply": tail_chars(reply, REPLY_CHARS),
        "messages_from_the_colleague_oldest_first": messages,
    })
}

pub fn compose(asked: &Asked, candidates: &[Candidate]) -> Linked {
    if asked.failed() {
        return Linked::Unanswered;
    }
    let Some((picked, probability)) = asked.choice(WHICH) else {
        return Linked::Unanswered;
    };
    if picked == NONE {
        return Linked::Nothing { probability };
    }
    candidates
        .iter()
        .enumerate()
        .find(|(at, _)| key(*at) == picked)
        .map_or(Linked::Unanswered, |(_, c)| Linked::Message {
            id: c.id,
            probability,
        })
}

pub async fn link(decider: &Decider, reply: &str, candidates: &[Candidate]) -> Linked {
    let asked = decider
        .ask(
            QUESTION_SET,
            state(reply, candidates),
            questions(candidates),
            DecisionSubject::default(),
            json!({}),
        )
        .await;
    let linked = compose(&asked, candidates);
    let (applied, score) = match &linked {
        Linked::Message { probability, .. } => ("chosen", *probability),
        Linked::Nothing { probability } => ("none", *probability),
        Linked::Unanswered => ("none", 1.0),
    };
    let applied = if decider.applies() { applied } else { "none" };
    decider
        .mark(&asked, asked.composed(json!({"score": score})), applied)
        .await;
    linked
}

pub fn answer_of(linked: &Linked, applies: bool) -> LinkAnswer {
    match linked {
        Linked::Message { id, probability } if applies => LinkAnswer {
            in_reply_to: Some(*id),
            decided_by: "jev",
            probability: Some(*probability),
        },
        Linked::Nothing { probability } if applies => LinkAnswer {
            in_reply_to: None,
            decided_by: "jev",
            probability: Some(*probability),
        },
        _ => LinkAnswer {
            in_reply_to: None,
            decided_by: "off",
            probability: None,
        },
    }
}

async fn candidates_of(
    state: &BaseState,
    session: Uuid,
    request: &LinkRequest,
) -> Result<Vec<Candidate>, StatusCode> {
    inbox_from(state, session, request.sender, Some(&request.candidates)).await
}

async fn inbox_from(
    state: &BaseState,
    session: Uuid,
    sender: Uuid,
    only: Option<&[Uuid]>,
) -> Result<Vec<Candidate>, StatusCode> {
    let persist = state.persist().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let rows = persist
        .launch()
        .fetch_for_session_with_activity(session)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let since = chrono::Utc::now() - chrono::Duration::hours(SEND_LOOKBACK_HOURS);
    let mut found: Vec<Candidate> = rows
        .inbox
        .into_iter()
        .map(|m| m.row)
        .filter(|row| row.from_session_uuid == Some(sender))
        .filter(|row| only.is_none_or(|ids| ids.contains(&row.message_uuid)))
        .filter(|row| only.is_some() || row.queued_at >= since)
        .map(|row| Candidate {
            id: row.message_uuid,
            queued_at: row.queued_at.to_rfc3339(),
            body: row.body,
        })
        .collect();
    found.sort_by(|a, b| a.queued_at.cmp(&b.queued_at));
    let from = found.len().saturating_sub(MAX_CANDIDATES);
    Ok(found.split_off(from))
}

pub async fn post_reply_link(
    State(state): State<BaseState>,
    Path(id): Path<Uuid>,
    Json(request): Json<LinkRequest>,
) -> Result<Json<LinkAnswer>, StatusCode> {
    let Some(decider) = state.decision_env().and_then(|env| env.decider(POINT)) else {
        return Ok(Json(answer_of(&Linked::Unanswered, false)));
    };
    let candidates = candidates_of(&state, id, &request).await?;
    if candidates.is_empty() {
        return Ok(Json(answer_of(&Linked::Unanswered, false)));
    }
    let linked = link(&decider, &request.reply, &candidates).await;
    Ok(Json(answer_of(&linked, decider.applies())))
}

pub async fn reply_target_on_send(
    state: &BaseState,
    replier: Uuid,
    recipient: Uuid,
    reply: &str,
) -> Option<Uuid> {
    let decider = state.decision_env()?.decider(POINT)?.inline();
    let candidates = inbox_from(state, replier, recipient, None).await.ok()?;
    if candidates.is_empty() {
        return None;
    }
    let linked = link(&decider, reply, &candidates).await;
    answer_of(&linked, decider.applies()).in_reply_to
}

#[cfg(test)]
#[path = "mail_reply_link_tests.rs"]
mod tests;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::analysis::judge::truncate_chars_tail;
use crate::store::persistence::judge::JudgeDigestRow;

#[derive(Debug, Deserialize)]
pub(crate) struct JudgeVerdict {
    pub(crate) outcome: String,
    pub(crate) quality_score: f64,
    pub(crate) rationale: String,
    pub(crate) overview: String,
    #[serde(default)]
    pub(crate) topics: Vec<String>,
    #[serde(default)]
    pub(crate) learnings: Vec<String>,
}

pub(crate) fn outcome_code_to_id(code: &str) -> i64 {
    match code {
        "success" => 2,
        "partial" => 3,
        "failure" => 4,
        _ => 1,
    }
}

pub(crate) use crate::settings::judge_prompts::SESSION_JUDGE_SYSTEM_PROMPT;

pub(crate) fn judge_verdict_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "outcome": {
                "type": "string",
                "enum": ["success", "partial", "failure", "unknown"],
                "description": "Overall outcome of the session."
            },
            "quality_score": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "Quality score in [0.0, 1.0] to three decimal places."
            },
            "rationale": {
                "type": "string",
                "description": "Short factual rationale for the verdict (1–3 sentences)."
            },
            "overview": {
                "type": "string",
                "description": "Narrative overview in exactly two paragraphs separated by a \
    blank line, no heading or bullets: paragraph one is what the session set out to do, \
    paragraph two is where it landed. Empty string when there is no material to summarize."
            },
            "topics": {
                "type": "array",
                "items": { "type": "string" },
                "maxItems": 5,
                "description": "3-5 short lower-case topic labels for this session. Empty \
    array only when there is no material to summarize."
            },
            "learnings": {
                "type": "array",
                "items": { "type": "string" },
                "maxItems": 5,
                "description": "0-5 durable claims this session established, learned, or \
    corrected. Empty array when nothing durable came out of the session."
            }
        },
        "required": ["outcome", "quality_score", "rationale", "overview", "topics", "learnings"],
        "additionalProperties": false
    })
}

pub(crate) fn build_digest(
    turns: &[JudgeDigestRow],
    max_turns: usize,
    max_chars_per_turn: usize,
) -> String {
    if turns.is_empty() {
        return "No operator turns found in this session.".to_string();
    }

    let mut out = String::with_capacity(turns.len() * 256);
    out.push_str("SESSION TURN DIGEST\n");
    out.push_str("===================\n\n");

    if turns.len() <= max_turns {
        for (i, turn) in turns.iter().enumerate() {
            push_digest_turn(&mut out, i + 1, turn, max_chars_per_turn);
        }
        return out;
    }

    push_digest_turn(&mut out, 1, &turns[0], max_chars_per_turn);

    let keep_recent = max_turns.saturating_sub(1);
    let recent_start = turns.len().saturating_sub(keep_recent);
    let omitted = recent_start - 1;
    if omitted > 0 {
        out.push_str(&format!("--- {omitted} earlier turn(s) omitted ---\n\n"));
    }

    for (i, turn) in turns[recent_start..].iter().enumerate() {
        push_digest_turn(&mut out, recent_start + i + 1, turn, max_chars_per_turn);
    }

    out
}

fn push_digest_turn(
    out: &mut String,
    seq: usize,
    turn: &JudgeDigestRow,
    max_chars_per_turn: usize,
) {
    out.push_str(&format!(
        "--- Turn {seq} (request_id={}) ---\n",
        turn.request_id
    ));
    out.push_str("USER PROMPT:\n");
    out.push_str(turn.user_prompt.trim());
    out.push('\n');

    if let Some(stop) = &turn.stop_response
        && !stop.trim().is_empty()
    {
        let trimmed = stop.trim();
        let capped = if max_chars_per_turn == 0 {
            trimmed
        } else {
            truncate_chars_tail(trimmed, max_chars_per_turn)
        };
        out.push_str("\nASSISTANT RESPONSE:\n");
        out.push_str(capped);
        out.push('\n');
    }
    out.push('\n');
}

pub(crate) struct RelatedSessionContext {
    pub(crate) title: Option<String>,
    pub(crate) last_seen: chrono::DateTime<chrono::Utc>,
    pub(crate) outcome: Option<String>,
    pub(crate) similarity: f64,
    pub(crate) overview: Option<String>,
    pub(crate) learnings: Vec<String>,
}

pub(crate) const RELATED_SESSION_OVERVIEW_MAX_CHARS: usize = 1_200;

pub(crate) const RELATED_SESSION_LEARNING_MAX_CHARS: usize = 300;

pub(crate) const RELATED_SESSIONS_BLOCK_MAX_CHARS: usize = 6_000;

pub(crate) fn build_related_sessions_block(related: &[RelatedSessionContext]) -> String {
    if related.is_empty() {
        return String::new();
    }
    const HEADER: &str = "\nRELATED SESSIONS\n=================\n\n";
    const FOOTER: &str = "If this session continues, repeats, or corrects the work of one of \
         the related sessions above, say so in the overview and name that session by its \
         title.\n";

    let mut entries: Vec<(usize, f64, String)> = related
        .iter()
        .enumerate()
        .map(|(i, r)| (i, r.similarity, render_related_session_entry(r)))
        .collect();

    let mut total: usize =
        HEADER.len() + FOOTER.len() + entries.iter().map(|(_, _, e)| e.len()).sum::<usize>();
    while entries.len() > 1 && total > RELATED_SESSIONS_BLOCK_MAX_CHARS {
        let mut drop_at = 0;
        for (i, &(_, similarity, _)) in entries.iter().enumerate() {
            if similarity < entries[drop_at].1 {
                drop_at = i;
            }
        }
        let (_, _, dropped) = entries.remove(drop_at);
        total -= dropped.len();
    }
    entries.sort_by_key(|&(original_pos, _, _)| original_pos);

    let mut out = String::with_capacity(total);
    out.push_str(HEADER);
    for (_, _, entry) in &entries {
        out.push_str(entry);
    }
    out.push_str(FOOTER);
    out
}

fn render_related_session_entry(r: &RelatedSessionContext) -> String {
    let mut out = String::with_capacity(256);
    let title = r.title.as_deref().unwrap_or("(untitled session)");
    let outcome = r.outcome.as_deref().unwrap_or("unknown");
    out.push_str(&format!(
        "- \"{title}\" (last seen {}, outcome: {outcome}, similarity {:.2})\n",
        r.last_seen.format("%Y-%m-%d"),
        r.similarity
    ));
    if let Some(overview) = r.overview.as_deref().filter(|o| !o.trim().is_empty()) {
        out.push_str("  Overview: ");
        out.push_str(truncate_chars_tail(
            overview.trim(),
            RELATED_SESSION_OVERVIEW_MAX_CHARS,
        ));
        out.push('\n');
    }
    if !r.learnings.is_empty() {
        out.push_str("  Learnings:\n");
        for learning in &r.learnings {
            out.push_str("    - ");
            out.push_str(truncate_chars_tail(
                learning,
                RELATED_SESSION_LEARNING_MAX_CHARS,
            ));
            out.push('\n');
        }
    }
    out.push('\n');
    out
}

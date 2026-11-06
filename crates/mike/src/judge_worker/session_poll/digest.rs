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

pub(crate) const SESSION_JUDGE_SYSTEM_PROMPT: &str = "\
You are a session-quality judge for an AI coding assistant. \
You will receive a digest of the operator turns in one session — \
each turn shows the user prompt and the assistant's final response. \
Your task is to assess the overall quality of the session AND write a \
narrative overview of what happened.\n\n\
Outcome definitions:\n\
- success: The session achieved its stated goal or made clear, useful progress.\n\
- partial: The session made some progress but left significant goals incomplete or \
introduced regressions.\n\
- failure: The session failed to achieve its goals, went in circles, or produced \
harmful/broken output.\n\
- unknown: There is insufficient information to judge (e.g. the session has no \
operator turns or only system turns).\n\n\
Quality score (0.0–1.0):\n\
- 1.0 = exemplary: clean, correct, complete work.\n\
- 0.7–0.9 = good: mostly successful with minor gaps.\n\
- 0.4–0.6 = partial: mixed results.\n\
- 0.1–0.3 = poor: mostly unsuccessful.\n\
- 0.0 = complete failure or no measurable output.\n\n\
Provide a short rationale (1–3 sentences). Be concise and factual.\n\n\
Overview: write a narrative overview of the session for a human reviewer. \
It MUST be EXACTLY two paragraphs, separated by a single blank line, with no \
heading, title, or bullet points — plain prose only.\n\n\
FIRST paragraph: what the work set out to do — the operator's original goal, \
in their terms. Describe the goal ONLY. Put no result, no progress and no \
outcome in this paragraph.\n\
SECOND paragraph: where it landed. What was finished, what was NOT, and the \
state it was left in. If something was left unfinished, blocked or abandoned, \
name it specifically; if nothing was, say so plainly.\n\n\
Style, all mandatory:\n\
- Make the work itself the subject of every sentence. Open the first \
paragraph with the goal as a noun phrase, e.g. \"Locating where consolidation \
runs are recorded...\" or \"A mailbox stranding bug that left 195 messages \
undelivered...\".\n\
- Open the second paragraph with the outcome as a noun phrase, e.g. \"The fix \
shipped and...\" or \"Two of the three call sites still...\".\n\
- Every sentence carries a specific fact: a file, a bug, a command, a \
component, an error message, a count. Where a number is available, use it.\n\
- Plain past tense, one clause per fact.\n\n\
If the digest gives you nothing to summarize (e.g. it reports no operator \
turns), return an EMPTY STRING for overview rather than inventing prose.\n\n\
Topics: 3-5 short, lower-case labels describing what this session was about \
(e.g. \"session judge\", \"embeddings\", \"bug fix\"). No punctuation, no \
capitalization. Empty array only when the digest gives you nothing to \
summarize.\n\n\
Learnings: 0-5 short sentences naming durable claims this session \
established, learned, or corrected — facts a future session working on the \
same area should know. Empty array when the session produced nothing \
durable.";

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

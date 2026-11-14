use chrono::{DateTime, Utc};

use super::{truncate_chars, truncate_chars_tail};

pub const INPUT_BUILDER_V2_0: &str = "judge-input-v2.0";
pub const INPUT_BUILDER_V2: &str = "judge-input-v2.1";
pub const INPUT_BUILDER_V2_SLICES: &str = "judge-input-v2.1+slices";
pub const INPUT_BUILDER_R3: &str = "judge-input-r3";

const PROMPT_CAP: usize = 1_000;
const ASSISTANT_CAP: usize = 3_000;
const NEXT_CAP: usize = 2_000;

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeTurnContext {
    pub turn_id: i64,
    pub session_id: i64,
    pub user_prompt: String,
    pub final_assistant_text: String,
    pub text_cut_at: i32,
    pub tool_call_count: i32,
    pub tool_error_count: i32,
    pub next_genuine_request_id: Option<i64>,
    pub next_genuine_text: Option<String>,
    pub next_genuine_at: Option<DateTime<Utc>>,
    pub closed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeInput {
    pub turn_id: i64,
    pub text: String,
    pub builder_version: &'static str,
    pub as_of: DateTime<Utc>,
    pub next_genuine_request_id: Option<i64>,
}

pub fn builder_for(version: &str) -> Option<&'static str> {
    match version {
        INPUT_BUILDER_V2_0 => Some(INPUT_BUILDER_V2_0),
        INPUT_BUILDER_V2 => Some(INPUT_BUILDER_V2),
        _ => None,
    }
}

fn assistant_text<'a>(ctx: &'a JudgeTurnContext, builder: &str) -> &'a str {
    if builder == INPUT_BUILDER_V2_0 {
        return &ctx.final_assistant_text;
    }
    let cut = usize::try_from(ctx.text_cut_at).unwrap_or(0);
    truncate_chars(&ctx.final_assistant_text, cut)
}

pub fn build_judge_input_v2(ctx: &JudgeTurnContext, builder: &'static str) -> JudgeInput {
    let prompt = truncate_chars(&ctx.user_prompt, PROMPT_CAP);
    let assistant = truncate_chars_tail(assistant_text(ctx, builder), ASSISTANT_CAP);
    let next = ctx
        .next_genuine_text
        .as_deref()
        .map_or("(none)", |text| truncate_chars(text, NEXT_CAP));
    let text = format!(
        "OPERATOR REQUEST (user prompt):\n\
         {prompt}\n\
         \n\
         ASSISTANT TURN TEXT (whole turn, in order; cut at the front when long):\n\
         {assistant}\n\
         \n\
         TOOL USE SUMMARY:\n\
         tool_call_count = {calls}\n\
         tool_error_count = {errors}\n\
         \n\
         OPERATOR'S NEXT GENUINE MESSAGE (automated events removed):\n\
         {next}",
        calls = ctx.tool_call_count,
        errors = ctx.tool_error_count,
    );
    JudgeInput {
        turn_id: ctx.turn_id,
        text,
        builder_version: builder,
        as_of: ctx.closed_at,
        next_genuine_request_id: ctx.next_genuine_request_id,
    }
}
